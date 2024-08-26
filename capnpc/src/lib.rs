// Copyright (c) 2013-2014 Sandstorm Development Group, Inc. and contributors
// Licensed under the MIT License:
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

//! # Cap'n Proto Schema Compiler Plugin Library
//!
//! This library allows you to do
//! [Cap'n Proto code generation](https://capnproto.org/otherlang.html#how-to-write-compiler-plugins)
//! within a Cargo build. This links against the capnpc-sys crate, so you don't need the capnp binary
//!
//! In your Cargo.toml:
//!
//! ```ignore
//! [dependencies]
//! capnp = "0.18" # Note this is a different library than capnp*c*
//!
//! [build-dependencies]
//! capnpc = "0.18"
//! ```
//!
//! In your build.rs:
//!
//! ```ignore
//! fn main() {
//!     capnpc::CompilerCommand::new()
//!         .src_prefix("schema")
//!         .file("schema/foo.capnp")
//!         .file("schema/bar.capnp")
//!         .run().expect("schema compiler command");
//! }
//! ```
//!
//! In your lib.rs:
//!
//! ```ignore
//! mod foo_capnp {
//!     include!(concat!(env!("OUT_DIR"), "/foo_capnp.rs"));
//! }
//!
//! mod bar_capnp {
//!     include!(concat!(env!("OUT_DIR"), "/bar_capnp.rs"));
//! }
//! ```

pub mod codegen;
pub mod codegen_types;
mod pointer_constants;
use convert_case::{Case, Casing};
use walkdir::WalkDir;
use wax::Glob;

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

// Copied from capnp/src/lib.rs, where this conversion lives behind the "std" feature flag,
// which we don't want to depend on here.
pub(crate) fn convert_io_err(err: std::io::Error) -> capnp::Error {
    use std::io;
    let kind = match err.kind() {
        io::ErrorKind::TimedOut => capnp::ErrorKind::Overloaded,
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::NotConnected => capnp::ErrorKind::Disconnected,
        _ => capnp::ErrorKind::Failed,
    };
    capnp::Error::from_kind_context(kind, format!("{err}"))
}

/// A builder object for schema compiler commands.
#[derive(Default)]
pub struct CompilerCommand {
    files: Vec<PathBuf>,
    src_prefixes: Vec<PathBuf>,
    import_paths: Vec<PathBuf>,
    no_standard_import: bool,
    output_path: Option<PathBuf>,
    default_parent_module: Vec<String>,
    raw_code_generator_request_path: Option<PathBuf>,
    crate_provides_map: HashMap<u64, String>,
    collect_file: Option<PathBuf>,
}

impl CompilerCommand {
    /// Creates a new, empty command.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file to be compiled.
    pub fn file<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.files.push(path.as_ref().to_path_buf());
        self
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Adds a --src-prefix flag. For all files specified for compilation that start
    /// with `prefix`, removes the prefix when computing output filenames.
    pub fn src_prefix<P>(&mut self, prefix: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.src_prefixes.push(prefix.as_ref().to_path_buf());
        self
    }

    /// Adds an --import_path flag. Adds `dir` to the list of directories searched
    /// for absolute imports.
    pub fn import_path<P>(&mut self, dir: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.import_paths.push(dir.as_ref().to_path_buf());
        self
    }

    /// Specify that `crate_name` provides generated code for `files`.
    ///
    /// This means that when your schema refers to types defined in `files` we
    /// will generate Rust code that uses identifiers in `crate_name`.
    ///
    /// # Arguments
    ///
    /// - `crate_name`: The Rust identifier of the crate
    /// - `files`: the Capnp file ids the crate provides generated code for
    ///
    /// # When to use
    ///
    /// You only need this when your generated code needs to refer to types in
    /// the external crate. If you just want to use an annotation and the
    /// argument to that annotation is a builtin type (e.g. `$Json.name`) this
    /// isn't necessary.
    ///
    /// # Example
    ///
    /// If you write a schema like so
    ///
    /// ```capnp
    /// // my_schema.capnp
    ///
    /// using Json = import "/capnp/compat/json.capnp";
    ///
    /// struct Foo {
    ///     value @0 :Json.Value;
    /// }
    /// ```
    ///
    /// you'd look at [json.capnp][json.capnp] to see its capnp id.
    ///
    /// ```capnp
    /// // json.capnp
    ///
    /// # Copyright (c) 2015 Sandstorm Development Group, Inc. and contributors ...
    /// @0x8ef99297a43a5e34;
    /// ```
    ///
    /// If you want the `foo::Builder::get_value` method generated for your
    /// schema to return a `capnp_json::json_capnp::value::Reader` you'd add a
    /// dependency on `capnp_json` to your `Cargo.toml` and specify it provides
    /// `json.capnp` in your `build.rs`.
    ///
    /// ```rust,no_run
    /// // build.rs
    ///
    /// capnpc::CompilerCommand::new()
    ///     .crate_provides("json_capnp", [0x8ef99297a43a5e34])
    ///     .file("my_schema.capnp")
    ///     .run()
    ///     .unwrap();
    /// ```
    ///
    /// [json.capnp]:
    ///     https://github.com/capnproto/capnproto/blob/master/c%2B%2B/src/capnp/compat/json.capnp
    pub fn crate_provides(
        &mut self,
        crate_name: impl Into<String>,
        files: impl IntoIterator<Item = u64>,
    ) -> &mut Self {
        let crate_name = crate_name.into();
        for file in files.into_iter() {
            self.crate_provides_map.insert(file, crate_name.clone());
        }
        self
    }

    /// Adds the --no-standard-import flag, indicating that the default import paths of
    /// /usr/include and /usr/local/include should not bet included.
    pub fn no_standard_import(&mut self) -> &mut Self {
        self.no_standard_import = true;
        self
    }

    /// Sets the output directory of generated code. Default is OUT_DIR
    pub fn output_path<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.output_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sets the default parent module. This indicates the scope in your crate where you will
    /// add a module containing the generated code. For example, if you set this option to
    /// `vec!["foo".into(), "bar".into()]`, and you are generating code for `baz.capnp`, then your crate
    /// should have this structure:
    ///
    /// ```ignore
    /// pub mod foo {
    ///    pub mod bar {
    ///        pub mod baz_capnp {
    ///            include!(concat!(env!("OUT_DIR"), "/baz_capnp.rs"));
    ///        }
    ///    }
    /// }
    /// ```
    ///
    /// This option can be overridden by the `parentModule` annotation defined in `rust.capnp`.
    ///
    /// If this option is unset, the default is the crate root.
    pub fn default_parent_module(&mut self, default_parent_module: Vec<String>) -> &mut Self {
        self.default_parent_module = default_parent_module;
        self
    }

    /// If set, the generator will also write a file containing the raw code generator request to the
    /// specified path.
    pub fn raw_code_generator_request_path<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.raw_code_generator_request_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Runs the command.
    /// Returns an error if `OUT_DIR` or a custom output directory was not set, or if `capnp compile` fails.
    pub fn run(&mut self) -> ::capnp::Result<()> {
        // We remove PWD from the env to avoid the following warning.
        // kj/filesystem-disk-unix.c++:1690:
        //    warning: PWD environment variable doesn't match current directory
        // command.env_remove("PWD");
        for file in &self.files {
            std::fs::metadata(file).map_err(|error| {
                let current_dir = match std::env::current_dir() {
                    Ok(current_dir) => format!("`{}`", current_dir.display()),
                    Err(..) => "<unknown working directory>".to_string(),
                };

                ::capnp::Error::failed(format!(
                    "Unable to stat capnp input file `{}` in working directory {}: {}.  \
                     Please check that the file exists and is accessible for read.",
                    file.display(),
                    current_dir,
                    error
                ))
            })?;
        }

        let output_path = if let Some(output_path) = &self.output_path {
            output_path.clone()
        } else {
            // Try `OUT_DIR` by default
            PathBuf::from(::std::env::var("OUT_DIR").map_err(|error| {
                ::capnp::Error::failed(format!(
                    "Could not access `OUT_DIR` environment variable: {error}. \
                     You might need to set it up or instead create you own output \
                     structure using `CompilerCommand::output_path`"
                ))
            })?)
        };

        let mut code_generation_command = crate::codegen::CodeGenerationCommand::new();
        code_generation_command
            .output_directory(output_path.clone())
            .default_parent_module(self.default_parent_module.clone())
            .crates_provide_map(self.crate_provides_map.clone());
        if let Some(raw_code_generator_request_path) = &self.raw_code_generator_request_path {
            code_generation_command
                .raw_code_generator_request_path(raw_code_generator_request_path.clone());
        }
        let output = capnpc_sys::call(
            self.files.iter().map(|p| p.display().to_string()),
            self.import_paths.iter().map(|p| p.display().to_string()),
            self.src_prefixes.iter().map(|p| p.display().to_string()),
            !self.no_standard_import,
        )
        .map_err(|e| capnp::Error::failed(e.to_string()))?;
        code_generation_command.run(output.as_slice())?;

        if let Some(omnibus) = self.collect_file.as_ref() {
            let mut f =
                std::fs::File::create(omnibus).map_err(|e| capnp::Error::failed(e.to_string()))?;
            for entry_result in WalkDir::new(output_path) {
                let file_path = entry_result
                    .map_err(|e| capnp::Error::failed(e.to_string()))?
                    .into_path();
                if file_path.is_file()
                    && file_path
                        .file_name()
                        .ok_or(capnp::Error::failed(format!(
                            "Couldn't parse file: {:?}",
                            file_path
                        )))?
                        .to_str()
                        .ok_or(capnp::Error::failed(format!(
                            "Couldn't convert to &str: {:?}",
                            file_path,
                        )))?
                        .ends_with("_capnp.rs")
                {
                    let file_stem = file_path
                        .file_stem()
                        .ok_or(capnp::Error::failed(format!(
                            "Couldn't parse file: {:?}",
                            file_path
                        )))?
                        .to_str()
                        .ok_or(capnp::Error::failed(format!(
                            "Couldn't convert to &str: {:?}",
                            file_path
                        )))?
                        .to_case(Case::Snake);

                    f.write_all(format!("pub mod {} {{", file_stem).as_bytes())
                        .map_err(|e| capnp::Error::failed(e.to_string()))?;
                    f.write_all(
                        std::fs::read_to_string(file_path)
                            .map_err(|e| capnp::Error::failed(e.to_string()))?
                            .as_bytes(),
                    )
                    .map_err(|e| capnp::Error::failed(e.to_string()))?;
                    f.write_all("\n}\n".as_bytes())
                        .map_err(|e| capnp::Error::failed(e.to_string()))?;
                }
            }
        }
        Ok(())
    }

    /// Automatically adds all files in `path_patterns`, either relative to the
    /// cargo manifest directory of the current project, or by looking in all
    /// searchable directories that were added via import_path().
    ///
    /// # Arguments
    ///
    /// - `path_patterns`: An array of valid wax::Glob path search patterns, as strings.
    ///
    /// # Example
    /// Adding all capnproto files in the current manifest directory under `/capnp/``:
    /// ```ignore
    /// self.add_paths(["capnp/*.capnp"]);
    /// ```
    ///
    /// Adding all capnproto files any import path in any subdirectory``:
    /// ```ignore
    /// self.add_paths(["/**/*.capnp"]);
    /// ```
    pub fn add_paths(&mut self, path_patterns: &[impl AsRef<str>]) -> ::capnp::Result<()> {
        let manifest: [PathBuf; 1] = [PathBuf::from_str(
            &std::env::var("CARGO_MANIFEST_DIR")
                .map_err(|e| capnp::Error::failed(e.to_string()))?,
        )
        .unwrap()];

        let search_paths: &[PathBuf] = &self.import_paths;
        let glob_matches = path_patterns
            .iter()
            .map(|pattern| -> ::capnp::Result<_> {
                let pattern = pattern.as_ref();
                let (search_prefix, glob) = Glob::new(pattern.trim_start_matches('/')).map_err(|e| ::capnp::Error::failed(e.to_string()))?.partition();
                Ok((pattern, search_prefix, glob))
            }).map(|maybe_pattern| {
                match maybe_pattern {
                    Ok((pattern, search_prefix, glob)) => {
                        let initial_paths = if pattern.starts_with('/')  { search_paths } else { &manifest };
                        let mut ensure_some = initial_paths
                        .iter()
                        .flat_map(move |dir: &PathBuf| -> _ {
                            // build glob and partition it into a static prefix and shorter glob pattern
                            // For example, converts "../schemas/*.capnp" into Path(../schemas) and Glob(*.capnp)
                            glob.walk(dir.join(&search_prefix)).into_owned().flatten()
                        }).peekable();
                        if ensure_some.peek().is_none() {
                            return Err(capnp::Error::failed(format!(
                                "No capnp files found matching {pattern}, did you mean to use an absolute path instead of a relative one?
                    Manifest directory for relative paths: {:#?}
                    Potential directories for absolute paths: {:#?}",
                                manifest,
                                search_paths
                            )));
                        }
                        Ok(ensure_some)
                    },
                    Err(err) => Err(err),
                }
            });

        for entry in glob_matches {
            for entry in entry? {
                if entry.file_type().is_file() {
                    self.files.push(entry.path().to_path_buf());
                }
            }
        }

        if self.file_count() == 0 {
            // I think the only way we can reach this now is by failing the is_file() check above
            return Err(::capnp::Error::failed(format!(
            "No capnp files found, did you mean to use an absolute path instead of a relative one?
  Manifest directory for relative paths: {:#?}
  Potential directories for absolute paths: {:#?}",
            manifest,
            search_paths
        )));
        }
        Ok(())
    }

    /// After compilation, collects all compiled files in the output directory
    /// into a single "omnibus" file created at the given path.
    ///
    /// # Arguments
    ///
    /// - `target`: Path where omnibus file should be created.
    pub fn omnibus<P: AsRef<Path>>(&mut self, target: P) -> &mut Self {
        self.collect_file.replace(target.as_ref().to_path_buf());
        self
    }
}

pub fn generate_random_id() -> u64 {
    capnpc_sys::id()
}
