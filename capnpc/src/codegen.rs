// Copyright (c) 2013-2015 Sandstorm Development Group, Inc. and contributors
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

use std::collections;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use capnp;
use capnp::Error;
use capnp::schema_capnp::{self, type_};

use self::FormattedText::{BlankLine, Branch, Indent, Line};
use crate::codegen_types::{Leaf, RustNodeInfo, RustTypeInfo, TypeParameterTexts, do_branding};
use crate::convert_io_err;
use crate::pointer_constants::generate_pointer_constant;

/// An invocation of the capnpc-rust code generation plugin.
pub struct CodeGenerationCommand {
    output_directory: PathBuf,
    default_parent_module: Vec<String>,
    raw_code_generator_request_path: Option<PathBuf>,
    capnp_root: String,
    crates_provide_map: HashMap<u64, String>,
}

impl Default for CodeGenerationCommand {
    fn default() -> Self {
        Self {
            output_directory: PathBuf::new(),
            default_parent_module: Vec::new(),
            raw_code_generator_request_path: None,
            capnp_root: "::capnp".into(),
            crates_provide_map: HashMap::new(),
        }
    }
}

impl CodeGenerationCommand {
    /// Creates a new code generation command with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the output directory.
    pub fn output_directory<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.output_directory = path.as_ref().to_path_buf();
        self
    }

    /// Sets the default parent module, indicating the scope in your crate where you will
    /// add the generated code.
    ///
    /// This option can be overridden by the `parentModule` annotation defined in `rust.capnp`.
    pub fn default_parent_module(&mut self, default_parent_module: Vec<String>) -> &mut Self {
        self.default_parent_module = default_parent_module;
        self
    }

    /// Sets the root path for referencing things in the `capnp` crate from the generated
    /// code. Usually this is `::capnp`. When we bootstrap schema.capnp we set this to `crate`.
    /// If you are renaming the `capnp` crate when importing it, then you probably want to set
    /// this value.
    pub fn capnp_root(&mut self, capnp_root: &str) -> &mut Self {
        self.capnp_root = capnp_root.into();
        self
    }

    /// Sets the raw code generator request output path.
    pub fn raw_code_generator_request_path<P>(&mut self, path: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.raw_code_generator_request_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sets the crate provides map.
    ///
    /// # Arguments
    ///
    /// - `map` - A map from capnp file id to the crate name that provides the
    ///   corresponding generated code.
    ///
    /// See [`crate::CompilerCommand::crate_provides`] for more details.
    pub fn crates_provide_map(&mut self, map: HashMap<u64, String>) -> &mut Self {
        self.crates_provide_map = map;
        self
    }

    /// Generates Rust code according to a `schema_capnp::code_generator_request` read from `inp`.
    pub fn run<T>(&mut self, inp: T) -> ::capnp::Result<()>
    where
        T: std::io::Read,
    {
        use capnp::serialize;
        use std::io::Write;

        let message = serialize::read_message(inp, capnp::message::ReaderOptions::new())?;

        let ctx = GeneratorContext::new_from_code_generation_command(self, &message)?;

        for requested_file in ctx.request.get_requested_files()? {
            let id = requested_file.get_id();
            let mut filepath = self.output_directory.to_path_buf();
            let requested = ::std::path::PathBuf::from(requested_file.get_filename()?.to_str()?);
            filepath.push(requested);
            if let Some(parent) = filepath.parent() {
                ::std::fs::create_dir_all(parent).map_err(convert_io_err)?;
            }

            let root_name = path_to_stem_string(&filepath)?.replace('-', "_");
            filepath.set_file_name(format!("{root_name}_capnp.rs"));

            let lines = Branch(vec![
                Line(
                    "// @generated by the capnpc-rust plugin to the Cap'n Proto schema compiler."
                        .to_string(),
                ),
                line("// DO NOT EDIT."),
                Line(format!(
                    "// source: {}",
                    requested_file.get_filename()?.to_str()?
                )),
                BlankLine,
                generate_node(
                    &ctx,
                    id,
                    &root_name,
                    &mut String::new(),
                    &mut String::new(),
                    &mut HashSet::new(),
                    &Vec::new(),
                    false,
                )?,
            ]);

            let text = stringify(&lines);

            let previous_text = ::std::fs::read(&filepath);
            if previous_text.is_ok() && previous_text.unwrap() == text.as_bytes() {
                // File is unchanged. Do not write it so that builds with the
                // output as part of the source work in read-only filesystems
                // and so timestamp-based build systems and watchers do not get
                // confused.
                continue;
            }

            // It would be simpler to use the ? operator instead of a pattern match, but then the error message
            // would not include `filepath`.
            match ::std::fs::File::create(&filepath) {
                Ok(mut writer) => {
                    writer.write_all(text.as_bytes()).map_err(convert_io_err)?;
                }
                Err(e) => {
                    let _ = writeln!(
                        &mut ::std::io::stderr(),
                        "could not open file {filepath:?} for writing: {e}"
                    );
                    return Err(convert_io_err(e));
                }
            }
        }

        if let Some(raw_code_generator_request) = &self.raw_code_generator_request_path {
            let raw_code_generator_request_file =
                ::std::fs::File::create(raw_code_generator_request).map_err(convert_io_err)?;
            serialize::write_message_segments(
                raw_code_generator_request_file,
                &message.into_segments(),
            )?;
        }

        Ok(())
    }
}

pub struct GeneratorContext<'a> {
    pub request: schema_capnp::code_generator_request::Reader<'a>,
    pub node_map: collections::hash_map::HashMap<u64, schema_capnp::node::Reader<'a>>,
    pub scope_map: collections::hash_map::HashMap<u64, Vec<String>>,

    /// Map from node ID to the node ID of its parent scope. This is equal to node.scope_id
    /// for all nodes except for autogenerated interface Param and Result structs;
    /// those have scope_id set to 0. See the comment on paramStructType in schema.capnp.
    pub node_parents: collections::hash_map::HashMap<u64, u64>,

    /// Root path for referencing things in the `capnp` crate from the generated code.
    pub capnp_root: String,
}

impl<'a> GeneratorContext<'a> {
    pub fn new(
        message: &'a capnp::message::Reader<capnp::serialize::OwnedSegments>,
    ) -> ::capnp::Result<GeneratorContext<'a>> {
        GeneratorContext::new_from_code_generation_command(&Default::default(), message)
    }

    fn new_from_code_generation_command(
        code_generation_command: &CodeGenerationCommand,
        message: &'a capnp::message::Reader<capnp::serialize::OwnedSegments>,
    ) -> ::capnp::Result<GeneratorContext<'a>> {
        let mut default_parent_module_scope = vec!["crate".to_string()];
        default_parent_module_scope
            .extend_from_slice(&code_generation_command.default_parent_module[..]);

        let mut ctx = GeneratorContext {
            request: message.get_root()?,
            node_map: collections::hash_map::HashMap::<u64, schema_capnp::node::Reader<'a>>::new(),
            scope_map: collections::hash_map::HashMap::<u64, Vec<String>>::new(),
            node_parents: collections::hash_map::HashMap::new(),
            capnp_root: code_generation_command.capnp_root.clone(),
        };

        let crates_provide = &code_generation_command.crates_provide_map;

        for node in ctx.request.get_nodes()? {
            ctx.node_map.insert(node.get_id(), node);
            ctx.node_parents.insert(node.get_id(), node.get_scope_id());
        }

        // Fix up "anonymous" method params and results scopes.
        for node in ctx.request.get_nodes()? {
            if let Ok(schema_capnp::node::Interface(interface_reader)) = node.which() {
                for method in interface_reader.get_methods()? {
                    let param_struct_type = method.get_param_struct_type();
                    if ctx.node_parents.get(&param_struct_type) == Some(&0) {
                        ctx.node_parents.insert(param_struct_type, node.get_id());
                    }
                    let result_struct_type = method.get_result_struct_type();
                    if ctx.node_parents.get(&result_struct_type) == Some(&0) {
                        ctx.node_parents.insert(result_struct_type, node.get_id());
                    }
                }
            }
        }

        for requested_file in ctx.request.get_requested_files()? {
            let id = requested_file.get_id();

            for import in requested_file.get_imports()? {
                let importpath = ::std::path::Path::new(import.get_name()?.to_str()?);
                let root_name: String = format!(
                    "{}_capnp",
                    path_to_stem_string(importpath)?.replace('-', "_")
                );
                let parent_module_scope = if let Some(krate) = crates_provide.get(&import.get_id())
                {
                    vec![format!("::{krate}")]
                } else {
                    default_parent_module_scope.clone()
                };

                ctx.populate_scope_map(
                    parent_module_scope,
                    root_name,
                    NameKind::Verbatim,
                    import.get_id(),
                )?;
            }

            let root_name = path_to_stem_string(requested_file.get_filename()?.to_str()?)?;
            let root_mod = format!("{}_capnp", root_name.replace('-', "_"));
            ctx.populate_scope_map(
                default_parent_module_scope.clone(),
                root_mod,
                NameKind::Verbatim,
                id,
            )?;
        }
        Ok(ctx)
    }

    fn get_last_name(&self, id: u64) -> ::capnp::Result<&str> {
        match self.scope_map.get(&id) {
            None => Err(Error::failed(format!("node not found: {id}"))),
            Some(v) => match v.last() {
                None => Err(Error::failed(format!("node has no scope: {id}"))),
                Some(n) => Ok(n),
            },
        }
    }

    fn populate_scope_map(
        &mut self,
        mut ancestor_scope_names: Vec<String>,
        mut current_node_name: String,
        current_name_kind: NameKind,
        node_id: u64,
    ) -> ::capnp::Result<()> {
        // unused nodes in imported files might be omitted from the node map
        let Some(&node_reader) = self.node_map.get(&node_id) else {
            return Ok(());
        };

        for annotation in node_reader.get_annotations()? {
            if annotation.get_id() == NAME_ANNOTATION_ID {
                current_node_name = name_annotation_value(annotation)?.to_string();
            } else if annotation.get_id() == PARENT_MODULE_ANNOTATION_ID {
                let head = ancestor_scope_names[0].clone();
                ancestor_scope_names.clear();
                ancestor_scope_names.push(head);
                ancestor_scope_names.append(&mut get_parent_module(annotation)?);
            }
        }

        let mut scope_names = ancestor_scope_names;
        scope_names.push(capnp_name_to_rust_name(
            &current_node_name,
            current_name_kind,
        ));

        self.scope_map.insert(node_id, scope_names.clone());

        let nested_nodes = node_reader.get_nested_nodes()?;
        for nested_node in nested_nodes {
            let nested_node_id = nested_node.get_id();
            match self.node_map.get(&nested_node_id) {
                None => {}
                Some(node_reader) => match node_reader.which() {
                    Ok(schema_capnp::node::Enum(_enum_reader)) => {
                        self.populate_scope_map(
                            scope_names.clone(),
                            nested_node.get_name()?.to_string()?,
                            NameKind::Verbatim,
                            nested_node_id,
                        )?;
                    }
                    _ => {
                        self.populate_scope_map(
                            scope_names.clone(),
                            nested_node.get_name()?.to_string()?,
                            NameKind::Module,
                            nested_node_id,
                        )?;
                    }
                },
            }
        }

        if let Ok(schema_capnp::node::Struct(struct_reader)) = node_reader.which() {
            let fields = struct_reader.get_fields()?;
            for field in fields {
                if let Ok(schema_capnp::field::Group(group)) = field.which() {
                    self.populate_scope_map(
                        scope_names.clone(),
                        get_field_name(field)?.to_string(),
                        NameKind::Module,
                        group.get_type_id(),
                    )?;
                }
            }
        }

        Ok(())
    }

    pub fn get_qualified_module(&self, type_id: u64) -> String {
        self.scope_map[&type_id].join("::")
    }
}

/// Like `format!(...)`, but adds a `capnp=ctx.capnp_root` argument.
macro_rules! fmt(
    ($ctx:ident, $($arg:tt)*) => ( format!($($arg)*, capnp=$ctx.capnp_root) )
);

pub(crate) use fmt;

fn path_to_stem_string<P: AsRef<::std::path::Path>>(path: P) -> ::capnp::Result<String> {
    match path.as_ref().file_stem() {
        None => Err(Error::failed(format!(
            "file has no stem: {:?}",
            path.as_ref()
        ))),
        Some(stem) => match stem.to_owned().into_string() {
            Err(os_string) => Err(Error::failed(format!("bad filename: {os_string:?}"))),
            Ok(s) => Ok(s),
        },
    }
}

fn snake_to_upper_case(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    for c in s.chars() {
        if c == '_' {
            result_chars.push('_');
        } else {
            result_chars.push(c.to_ascii_uppercase());
        }
    }
    result_chars.into_iter().collect()
}

fn snake_to_camel_case(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    let mut capitalize = true;
    for c in s.chars() {
        if capitalize {
            result_chars.push(c.to_ascii_uppercase());
            capitalize = false;
        } else if c == '_' {
            capitalize = true;
        } else {
            result_chars.push(c);
        }
    }
    result_chars.into_iter().collect()
}

fn camel_to_snake_case(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    let mut first_char = true;
    for c in s.chars() {
        if c.is_uppercase() && !first_char {
            result_chars.push('_');
        }
        result_chars.push(c.to_ascii_lowercase());
        first_char = false;
    }
    result_chars.into_iter().collect()
}

fn capitalize_first_letter(s: &str) -> String {
    let mut result_chars: Vec<char> = Vec::new();
    for c in s.chars() {
        result_chars.push(c)
    }
    result_chars[0] = result_chars[0].to_ascii_uppercase();
    result_chars.into_iter().collect()
}

/// Formats a u64 into a string representation of the hex value, with
/// separating underscores. Used instead of simple hex formatting to prevent
/// clippy warnings in autogenerated code. This is loosely based off of
/// similar functionality in the `separator` crate.
fn format_u64(value: u64) -> String {
    let hex = format!("{value:#x}");
    let mut separated = hex[0..2].to_string();
    let mut place = hex.len() - 2;
    let mut later_loop = false;

    for ch in hex[2..].chars() {
        if later_loop && place % 4 == 0 {
            separated.push('_');
        }

        separated.push(ch);
        later_loop = true;
        place -= 1;
    }

    separated
}

#[test]
fn test_camel_to_snake_case() {
    assert_eq!(camel_to_snake_case("fooBar"), "foo_bar".to_string());
    assert_eq!(camel_to_snake_case("FooBar"), "foo_bar".to_string());
    assert_eq!(camel_to_snake_case("fooBarBaz"), "foo_bar_baz".to_string());
    assert_eq!(camel_to_snake_case("FooBarBaz"), "foo_bar_baz".to_string());
    assert_eq!(camel_to_snake_case("helloWorld"), "hello_world".to_string());
    assert_eq!(camel_to_snake_case("HelloWorld"), "hello_world".to_string());
    assert_eq!(camel_to_snake_case("uint32Id"), "uint32_id".to_string());

    assert_eq!(camel_to_snake_case("fooBar_"), "foo_bar_".to_string());
}

#[derive(PartialEq, Clone)]
pub enum FormattedText {
    Indent(Box<FormattedText>),
    Branch(Vec<FormattedText>),
    Line(String),
    BlankLine,
}

impl From<Vec<FormattedText>> for FormattedText {
    fn from(value: Vec<FormattedText>) -> Self {
        Branch(value)
    }
}

pub fn indent(inner: impl Into<FormattedText>) -> FormattedText {
    Indent(Box::new(inner.into()))
}

pub fn line(inner: impl ToString) -> FormattedText {
    Line(inner.to_string())
}

fn to_lines(ft: &FormattedText, indent: usize) -> Vec<String> {
    match ft {
        Indent(ft) => to_lines(ft, indent + 1),
        Branch(fts) => {
            let mut result = Vec::new();
            for ft in fts {
                for line in &to_lines(ft, indent) {
                    result.push(line.clone()); // TODO there's probably a better way to do this.
                }
            }
            result
        }
        Line(s) => {
            let mut s1: String = " ".repeat(indent * 2);
            s1.push_str(s);
            vec![s1.to_string()]
        }
        BlankLine => vec!["".to_string()],
    }
}

fn stringify(ft: &FormattedText) -> String {
    let mut result = to_lines(ft, 0).join("\n");
    result.push('\n');
    result.to_string()
}

const RUST_KEYWORDS: [&str; 53] = [
    "abstract", "alignof", "as", "be", "become", "box", "break", "const", "continue", "crate",
    "do", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "offsetof", "once", "override", "priv", "proc",
    "pub", "pure", "ref", "return", "self", "sizeof", "static", "struct", "super", "trait", "true",
    "type", "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

fn module_name(camel_case: &str) -> String {
    let mut name = camel_to_snake_case(camel_case);
    if RUST_KEYWORDS.contains(&&*name) {
        name.push('_');
    }
    name
}

// Annotation IDs, as defined in rust.capnp.
const NAME_ANNOTATION_ID: u64 = 0xc2fe4c6d100166d0;
const PARENT_MODULE_ANNOTATION_ID: u64 = 0xabee386cd1450364;
const OPTION_ANNOTATION_ID: u64 = 0xabfef22c4ee1964e;

fn name_annotation_value(annotation: schema_capnp::annotation::Reader) -> capnp::Result<&str> {
    if let schema_capnp::value::Text(t) = annotation.get_value()?.which()? {
        let name = t?.to_str()?;
        for c in name.chars() {
            if !(c == '_' || c.is_alphanumeric()) {
                return Err(capnp::Error::failed(
                    "rust.name annotation value must only contain alphanumeric characters and '_'"
                        .to_string(),
                ));
            }
        }
        Ok(name)
    } else {
        Err(capnp::Error::failed(
            "expected rust.name annotation value to be of type Text".to_string(),
        ))
    }
}

fn get_field_name(field: schema_capnp::field::Reader) -> capnp::Result<&str> {
    for annotation in field.get_annotations()? {
        if annotation.get_id() == NAME_ANNOTATION_ID {
            return name_annotation_value(annotation);
        }
    }
    Ok(field.get_name()?.to_str()?)
}

fn get_enumerant_name(enumerant: schema_capnp::enumerant::Reader) -> capnp::Result<&str> {
    for annotation in enumerant.get_annotations()? {
        if annotation.get_id() == NAME_ANNOTATION_ID {
            return name_annotation_value(annotation);
        }
    }
    Ok(enumerant.get_name()?.to_str()?)
}

fn get_parent_module(annotation: schema_capnp::annotation::Reader) -> capnp::Result<Vec<String>> {
    if let schema_capnp::value::Text(t) = annotation.get_value()?.which()? {
        let module = t?.to_str()?;
        Ok(module.split("::").map(|x| x.to_string()).collect())
    } else {
        Err(capnp::Error::failed(
            "expected rust.parentModule annotation value to be of type Text".to_string(),
        ))
    }
}
#[derive(Clone, Copy)]
enum NameKind {
    // convert camel case to snake case, and avoid Rust keywords
    Module,

    // don't modify
    Verbatim,
}

fn capnp_name_to_rust_name(capnp_name: &str, name_kind: NameKind) -> String {
    match name_kind {
        NameKind::Module => module_name(capnp_name),
        NameKind::Verbatim => capnp_name.to_string(),
    }
}

fn is_option_field(field: schema_capnp::field::Reader) -> capnp::Result<bool> {
    use capnp::schema_capnp::*;

    let enabled = field
        .get_annotations()?
        .iter()
        .any(|a| a.get_id() == OPTION_ANNOTATION_ID);

    if enabled {
        let supported = match field.which()? {
            field::Which::Group(_) => false,
            field::Which::Slot(field) => {
                let ty = field.get_type()?;
                ty.is_pointer()? && !matches!(ty.which()?, type_::Interface(_))
            }
        };
        if !supported {
            return Err(capnp::Error::failed(
                "$Rust.option annotation only supported on pointer fields (support for optional interfaces isn't implemented yet)".to_string(),
            ));
        }
    }

    Ok(enabled)
}

fn prim_default(value: &schema_capnp::value::Reader) -> ::capnp::Result<Option<String>> {
    use capnp::schema_capnp::value;
    match value.which()? {
        value::Bool(false)
        | value::Int8(0)
        | value::Int16(0)
        | value::Int32(0)
        | value::Int64(0)
        | value::Uint8(0)
        | value::Uint16(0)
        | value::Uint32(0)
        | value::Uint64(0) => Ok(None),

        value::Bool(true) => Ok(Some("true".to_string())),
        value::Int8(i) => Ok(Some(i.to_string())),
        value::Int16(i) => Ok(Some(i.to_string())),
        value::Int32(i) => Ok(Some(i.to_string())),
        value::Int64(i) => Ok(Some(i.to_string())),
        value::Uint8(i) => Ok(Some(i.to_string())),
        value::Uint16(i) => Ok(Some(i.to_string())),
        value::Uint32(i) => Ok(Some(i.to_string())),
        value::Uint64(i) => Ok(Some(i.to_string())),
        value::Float32(f) => match f.classify() {
            ::std::num::FpCategory::Zero => Ok(None),
            _ => Ok(Some(format!("{}u32", f.to_bits()))),
        },
        value::Float64(f) => match f.classify() {
            ::std::num::FpCategory::Zero => Ok(None),
            _ => Ok(Some(format!("{}u64", f.to_bits()))),
        },
        _ => Err(Error::failed(
            "Non-primitive value found where primitive was expected.".to_string(),
        )),
    }
}

// Gets the full list ordered of generic parameters for a node. Outer scopes come first.
fn get_params(ctx: &GeneratorContext, mut node_id: u64) -> ::capnp::Result<Vec<String>> {
    let mut result = Vec::new();

    while node_id != 0 {
        let node = ctx.node_map[&node_id];
        let parameters = node.get_parameters()?;

        for parameter in parameters.into_iter().rev() {
            result.push(parameter.get_name()?.to_str()?.into());
        }

        node_id = node.get_scope_id();
    }

    result.reverse();
    Ok(result)
}

//
// Returns (type, getter body, default_decl)
//
pub fn getter_text(
    ctx: &GeneratorContext,
    field: &schema_capnp::field::Reader,
    is_reader: bool,
    is_fn: bool,
) -> ::capnp::Result<(String, FormattedText, Option<FormattedText>)> {
    use capnp::schema_capnp::*;

    match field.which()? {
        field::Group(group) => {
            let params = get_params(ctx, group.get_type_id())?;
            let params_string = if params.is_empty() {
                "".to_string()
            } else {
                format!(",{}", params.join(","))
            };

            let the_mod = ctx.get_qualified_module(group.get_type_id());

            let mut result_type = if is_reader {
                format!("{the_mod}::Reader<'a{params_string}>")
            } else {
                format!("{the_mod}::Builder<'a{params_string}>")
            };

            if is_fn {
                result_type = format!("-> {result_type}");
            }

            let getter_code = if is_reader {
                line("self.reader.into()")
            } else {
                line("self.builder.into()")
            };

            Ok((result_type, getter_code, None))
        }
        field::Slot(reg_field) => {
            let mut default_decl = None;
            let offset = reg_field.get_offset() as usize;
            let module_string = if is_reader { "Reader" } else { "Builder" };
            let module = if is_reader {
                Leaf::Reader("'a")
            } else {
                Leaf::Builder("'a")
            };
            let member = camel_to_snake_case(module_string);

            fn primitive_case<T: PartialEq + ::std::fmt::Display>(
                typ: &str,
                member: &str,
                offset: usize,
                default: T,
                zero: T,
            ) -> String {
                if default == zero {
                    format!("self.{member}.get_data_field::<{typ}>({offset})")
                } else {
                    format!("self.{member}.get_data_field_mask::<{typ}>({offset}, {default})")
                }
            }

            let raw_type = reg_field.get_type()?;
            let inner_type = raw_type.type_string(ctx, module)?;
            let default_value = reg_field.get_default_value()?;
            let default = default_value.which()?;
            let default_name = format!(
                "DEFAULT_{}",
                snake_to_upper_case(&camel_to_snake_case(get_field_name(*field)?))
            );
            let should_get_option = is_option_field(*field)?;

            let typ = if should_get_option {
                format!("Option<{inner_type}>",)
            } else {
                inner_type
            };

            let (is_fallible, mut result_type) = match raw_type.which()? {
                type_::Enum(_) => (
                    true,
                    fmt!(ctx, "::core::result::Result<{typ},{capnp}::NotInSchema>"),
                ),
                type_::AnyPointer(_) if !raw_type.is_parameter()? => (false, typ.clone()),
                type_::Interface(_) => (
                    true,
                    fmt!(
                        ctx,
                        "{capnp}::Result<{}>",
                        raw_type.type_string(ctx, Leaf::Client)?
                    ),
                ),
                _ if raw_type.is_prim()? => (false, typ.clone()),
                _ => (true, fmt!(ctx, "{capnp}::Result<{typ}>")),
            };

            if is_fn {
                result_type = if result_type == "()" {
                    "".to_string()
                } else {
                    format!("-> {result_type}")
                }
            }

            let getter_fragment = match (raw_type.which()?, default) {
                (type_::Void(()), value::Void(())) => {
                    if is_fn {
                        "".to_string()
                    } else {
                        "()".to_string()
                    }
                }
                (type_::Bool(()), value::Bool(b)) => {
                    if b {
                        format!("self.{member}.get_bool_field_mask({offset}, true)")
                    } else {
                        format!("self.{member}.get_bool_field({offset})")
                    }
                }
                (type_::Int8(()), value::Int8(i)) => primitive_case(&typ, &member, offset, i, 0),
                (type_::Int16(()), value::Int16(i)) => primitive_case(&typ, &member, offset, i, 0),
                (type_::Int32(()), value::Int32(i)) => primitive_case(&typ, &member, offset, i, 0),
                (type_::Int64(()), value::Int64(i)) => primitive_case(&typ, &member, offset, i, 0),
                (type_::Uint8(()), value::Uint8(i)) => primitive_case(&typ, &member, offset, i, 0),
                (type_::Uint16(()), value::Uint16(i)) => {
                    primitive_case(&typ, &member, offset, i, 0)
                }
                (type_::Uint32(()), value::Uint32(i)) => {
                    primitive_case(&typ, &member, offset, i, 0)
                }
                (type_::Uint64(()), value::Uint64(i)) => {
                    primitive_case(&typ, &member, offset, i, 0)
                }
                (type_::Float32(()), value::Float32(f)) => {
                    primitive_case(&typ, &member, offset, f.to_bits(), 0)
                }
                (type_::Float64(()), value::Float64(f)) => {
                    primitive_case(&typ, &member, offset, f.to_bits(), 0)
                }
                (type_::Enum(_), value::Enum(d)) => {
                    if d == 0 {
                        format!(
                            "::core::convert::TryInto::try_into(self.{member}.get_data_field::<u16>({offset}))"
                        )
                    } else {
                        format!(
                            "::core::convert::TryInto::try_into(self.{member}.get_data_field_mask::<u16>({offset}, {d}))"
                        )
                    }
                }

                (type_::Text(()), value::Text(_))
                | (type_::Data(()), value::Data(_))
                | (type_::List(_), value::List(_))
                | (type_::Struct(_), value::Struct(_)) => {
                    let default = if reg_field.get_had_explicit_default() {
                        default_decl = Some(crate::pointer_constants::word_array_declaration(
                            ctx,
                            &default_name,
                            ::capnp::raw::get_struct_pointer_section(default_value).get(0),
                            crate::pointer_constants::WordArrayDeclarationOptions { public: true },
                        )?);
                        format!("::core::option::Option::Some(&_private::{default_name}[..])")
                    } else {
                        "::core::option::Option::None".to_string()
                    };

                    if is_reader {
                        fmt!(
                            ctx,
                            "{capnp}::traits::FromPointerReader::get_from_pointer(&self.{member}.get_pointer_field({offset}), {default})"
                        )
                    } else {
                        fmt!(
                            ctx,
                            "{capnp}::traits::FromPointerBuilder::get_from_pointer(self.{member}.get_pointer_field({offset}), {default})"
                        )
                    }
                }

                (type_::Interface(_), value::Interface(_)) => {
                    fmt!(
                        ctx,
                        "match self.{member}.get_pointer_field({offset}).get_capability() {{ ::core::result::Result::Ok(c) => ::core::result::Result::Ok({capnp}::capability::FromClientHook::new(c)), ::core::result::Result::Err(e) => ::core::result::Result::Err(e)}}"
                    )
                }
                (type_::AnyPointer(_), value::AnyPointer(_)) => {
                    if !raw_type.is_parameter()? {
                        fmt!(
                            ctx,
                            "{capnp}::any_pointer::{module_string}::new(self.{member}.get_pointer_field({offset}))"
                        )
                    } else if is_reader {
                        fmt!(
                            ctx,
                            "{capnp}::traits::FromPointerReader::get_from_pointer(&self.{member}.get_pointer_field({offset}), ::core::option::Option::None)"
                        )
                    } else {
                        fmt!(
                            ctx,
                            "{capnp}::traits::FromPointerBuilder::get_from_pointer(self.{member}.get_pointer_field({offset}), ::core::option::Option::None)"
                        )
                    }
                }
                _ => return Err(Error::failed("default value was of wrong type".to_string())),
            };

            let getter_code = if should_get_option {
                Branch(vec![
                    Line(format!(
                        "if self.{member}.is_pointer_field_null({offset}) {{"
                    )),
                    indent(Line(
                        if is_fallible {
                            "core::result::Result::Ok(core::option::Option::None)"
                        } else {
                            "::core::option::Option::None"
                        }
                        .to_string(),
                    )),
                    Line("} else {".to_string()),
                    indent(Line(if is_fallible {
                        format!("{getter_fragment}.map(::core::option::Option::Some)")
                    } else {
                        format!("::core::option::Option::Some({getter_fragment})")
                    })),
                    Line("}".to_string()),
                ])
            } else {
                Line(getter_fragment)
            };

            Ok((result_type, getter_code, default_decl))
        }
    }
}

fn zero_fields_of_group(
    ctx: &GeneratorContext,
    node_id: u64,
    clear: &mut bool,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::{field, node};
    match ctx.node_map[&node_id].which()? {
        node::Struct(st) => {
            let mut result = Vec::new();
            if st.get_discriminant_count() != 0 {
                result.push(Line(format!(
                    "self.builder.set_data_field::<u16>({}, 0);",
                    st.get_discriminant_offset()
                )));
            }
            let fields = st.get_fields()?;
            for field in fields {
                match field.which()? {
                    field::Group(group) => {
                        result.push(zero_fields_of_group(ctx, group.get_type_id(), clear)?);
                    }
                    field::Slot(slot) => {
                        let typ = slot.get_type()?.which()?;
                        match typ {
                            type_::Void(()) => {}
                            type_::Bool(()) => {
                                let line = Line(format!(
                                    "self.builder.set_bool_field({}, false);",
                                    slot.get_offset()
                                ));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) {
                                    result.push(line)
                                }
                            }
                            type_::Int8(())
                            | type_::Int16(())
                            | type_::Int32(())
                            | type_::Int64(())
                            | type_::Uint8(())
                            | type_::Uint16(())
                            | type_::Uint32(())
                            | type_::Uint64(())
                            | type_::Float32(())
                            | type_::Float64(()) => {
                                let line = Line(format!(
                                    "self.builder.set_data_field::<{0}>({1}, 0{0});",
                                    slot.get_type()?.type_string(ctx, Leaf::Builder("'a"))?,
                                    slot.get_offset()
                                ));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) {
                                    result.push(line)
                                }
                            }
                            type_::Enum(_) => {
                                let line = Line(format!(
                                    "self.builder.set_data_field::<u16>({}, 0u16);",
                                    slot.get_offset()
                                ));
                                // PERF could dedup more efficiently
                                if !result.contains(&line) {
                                    result.push(line)
                                }
                            }
                            type_::Struct(_)
                            | type_::List(_)
                            | type_::Text(())
                            | type_::Data(())
                            | type_::AnyPointer(_)
                            | type_::Interface(_) => {
                                // Is this the right thing to do for interfaces?
                                let line = Line(format!(
                                    "self.builder.reborrow().get_pointer_field({}).clear();",
                                    slot.get_offset()
                                ));
                                *clear = true;
                                // PERF could dedup more efficiently
                                if !result.contains(&line) {
                                    result.push(line)
                                }
                            }
                        }
                    }
                }
            }
            Ok(Branch(result))
        }
        _ => Err(Error::failed(
            "zero_fields_of_groupd() expected a struct".to_string(),
        )),
    }
}

fn generate_setter(
    ctx: &GeneratorContext,
    discriminant_offset: u32,
    styled_name: &str,
    field: &schema_capnp::field::Reader,
    rust_struct_inner: &mut String,
    rust_struct_impl_inner: &mut String,
    set_types: &mut String,
    set_inner: &mut String,
    is_params_struct: bool,
    params_struct_generics: &mut HashSet<String>,
    interface_implicit_generics: &[String],
    node_name: &str,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::*;

    let params_struct_prefix = if is_params_struct { " " } else { "\n pub " };
    let params_struct_impl_prefix = if is_params_struct { "" } else { "self." };
    let mut setter_interior = Vec::new();
    let mut setter_param = "value".to_string();
    let mut initter_interior = Vec::new();
    let mut initter_mut = false;
    let mut initn_interior = Vec::new();
    let mut initter_params = Vec::new();
    let mut no_discriminant = true;

    let discriminant_value = field.get_discriminant_value();
    if discriminant_value != field::NO_DISCRIMINANT {
        no_discriminant = false;
        setter_interior.push(Line(format!(
            "self.builder.set_data_field::<u16>({}, {});",
            discriminant_offset as usize, discriminant_value as usize
        )));
        let init_discrim = Line(format!(
            "self.builder.set_data_field::<u16>({}, {});",
            discriminant_offset as usize, discriminant_value as usize
        ));
        initter_interior.push(init_discrim.clone());
        initn_interior.push(init_discrim);
    }

    let mut return_result = false;
    let mut result = Vec::new();

    let (maybe_reader_type, maybe_builder_type): (Option<String>, Option<String>) = match field
        .which()?
    {
        field::Group(group) => {
            let the_mod = ctx.get_qualified_module(group.get_type_id());
            let params = get_params(ctx, group.get_type_id())?;
            let mut used_params = HashSet::new();
            used_params_of_group(ctx, group.get_type_id(), &mut used_params)?;
            for par in &used_params {
                params_struct_generics.insert(par.to_string());
            }
            if no_discriminant {
                let mut lifetime = "";
                let node::Struct(struct_node) = ctx.node_map[&group.get_type_id()].which()? else {
                    return Err(capnp::Error::failed("Type mismatch".to_string()));
                };
                check_fields_of_struct_for_lifetimes(
                    ctx,
                    struct_node.get_fields()?,
                    &mut lifetime,
                    0,
                )?;
                if !lifetime.is_empty() {
                    params_struct_generics.insert("'a".to_string());
                }
                let bracketed_params = if !lifetime.is_empty() || !used_params.is_empty() {
                    format! {"<{lifetime}{}>", used_params.into_iter().collect::<Vec<String>>().join(",")}
                } else {
                    "".to_string()
                };
                set_types.push_str(
                    format!(
                        ", _{styled_name}: {}::{}{bracketed_params}",
                        the_mod,
                        snake_to_camel_case(ctx.get_last_name(group.get_type_id())?)
                    )
                    .as_str(),
                );
                set_inner.push_str(format!("\n  _{styled_name}.build_capnp_struct(self.reborrow().init_{styled_name}());").as_str());
                rust_struct_inner.push_str(
                    format!(
                        "{params_struct_prefix}_{styled_name}: {}::{}{bracketed_params},",
                        the_mod,
                        snake_to_camel_case(ctx.get_last_name(group.get_type_id())?)
                    )
                    .as_str(),
                );
                rust_struct_impl_inner.push_str(format!("\n  {params_struct_impl_prefix}_{styled_name}.build_capnp_struct(_builder.reborrow().init_{styled_name}());").as_str());
            }
            let params_string = if params.is_empty() {
                "".to_string()
            } else {
                format!(",{}", params.join(","))
            };

            initter_interior.push(zero_fields_of_group(
                ctx,
                group.get_type_id(),
                &mut initter_mut,
            )?);

            initter_interior.push(line("self.builder.into()"));

            (None, Some(format!("{the_mod}::Builder<'a{params_string}>")))
        }
        field::Slot(reg_field) => {
            let offset = reg_field.get_offset() as usize;
            let typ = reg_field.get_type()?;
            let mut used_params = HashSet::new();
            used_params_of_type(ctx, typ.reborrow(), &mut used_params)?;
            for par in &used_params {
                params_struct_generics.insert(par.to_string());
            }
            match typ.which().expect("unrecognized type") {
                type_::Void(()) => {
                    setter_param = "_value".to_string();
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: ()").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name});").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: (),").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name});").as_str());
                    }
                    (Some("()".to_string()), None)
                }
                type_::Bool(()) => {
                    match prim_default(&reg_field.get_default_value()?)? {
                        None => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_bool_field({offset}, value);"
                            )));
                        }
                        Some(s) => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_bool_field_mask({offset}, value, {s});"
                            )));
                        }
                    }
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: bool").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name});").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: bool,").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name});").as_str());
                    }
                    (Some("bool".to_string()), None)
                }
                _ if typ.is_prim()? => {
                    let tstr = typ.type_string(ctx, Leaf::Reader("'a"))?;
                    match prim_default(&reg_field.get_default_value()?)? {
                        None => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_data_field::<{tstr}>({offset}, value);"
                            )));
                        }
                        Some(s) => {
                            setter_interior.push(Line(format!(
                                "self.builder.set_data_field_mask::<{tstr}>({offset}, value, {s});"
                            )));
                        }
                    };
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: {tstr}").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name});").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: {tstr},").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name});").as_str());
                    }
                    (Some(tstr), None)
                }
                type_::Text(()) => {
                    params_struct_generics.insert("'a".to_string());
                    setter_interior.push(Line(format!(
                        "self.builder.reborrow().get_pointer_field({offset}).set_text(value);"
                    )));
                    initter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({offset}).init_text(size)"
                    )));
                    initter_params.push("size: u32");
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: &str").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name}.into());").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: &'a str,").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name}.into());").as_str());
                    }
                    (
                        Some(fmt!(ctx, "{capnp}::text::Reader<'_>")),
                        Some(fmt!(ctx, "{capnp}::text::Builder<'a>")),
                    )
                }
                type_::Data(()) => {
                    params_struct_generics.insert("'a".to_string());
                    setter_interior.push(Line(format!(
                        "self.builder.reborrow().get_pointer_field({offset}).set_data(value);"
                    )));
                    initter_interior.push(Line(format!(
                        "self.builder.get_pointer_field({offset}).init_data(size)"
                    )));
                    initter_params.push("size: u32");
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: &[u8]").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name}.into());").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: &'a [u8],").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name});").as_str());
                    }
                    (
                        Some(fmt!(ctx, "{capnp}::data::Reader<'_>")),
                        Some(fmt!(ctx, "{capnp}::data::Builder<'a>")),
                    )
                }
                type_::List(ot1) => {
                    return_result = true;
                    setter_interior.push(
                        Line(fmt!(ctx,"{capnp}::traits::SetPointerBuilder::set_pointer_builder(self.builder.reborrow().get_pointer_field({offset}), value, false)")));

                    initter_params.push("size: u32");
                    initter_interior.push(
                        Line(fmt!(ctx,"{capnp}::traits::FromPointerBuilder::init_pointer(self.builder.get_pointer_field({offset}), size)")));

                    if no_discriminant {
                        if let Ok(vec_of_list_element_types) =
                            vec_of_list_element_types(ctx, ot1.reborrow(), params_struct_generics)
                        {
                            set_types.push_str(
                                format!(", _{styled_name}: {vec_of_list_element_types}").as_str(),
                            );
                            set_inner.push_str(
                                build_impl_for_list_type(
                                    styled_name,
                                    "self",
                                    ot1.reborrow(),
                                    false,
                                    true,
                                )?
                                .as_str(),
                            );
                            rust_struct_inner.push_str(
                                format!(
                                    "{params_struct_prefix}_{styled_name}: {vec_of_list_element_types},"
                                )
                                .as_str(),
                            );
                            rust_struct_impl_inner.push_str(
                                build_impl_for_list_type(
                                    styled_name,
                                    "_builder",
                                    ot1.reborrow(),
                                    false,
                                    is_params_struct,
                                )?
                                .as_str(),
                            );
                        }
                    }

                    match ot1.get_element_type()?.which()? {
                        type_::List(_) => (
                            Some(reg_field.get_type()?.type_string(ctx, Leaf::Reader("'_"))?),
                            Some(
                                reg_field
                                    .get_type()?
                                    .type_string(ctx, Leaf::Builder("'a"))?,
                            ),
                        ),
                        _ => (
                            Some(reg_field.get_type()?.type_string(ctx, Leaf::Reader("'a"))?),
                            Some(
                                reg_field
                                    .get_type()?
                                    .type_string(ctx, Leaf::Builder("'a"))?,
                            ),
                        ),
                    }
                }
                type_::Enum(e) => {
                    let id = e.get_type_id();
                    let the_mod = ctx.get_qualified_module(id);
                    if no_discriminant {
                        set_types.push_str(format!(", _{styled_name}: {the_mod}").as_str());
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name});").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!("{params_struct_prefix}_{styled_name}: {the_mod},").as_str(),
                        );
                        rust_struct_impl_inner.push_str(format!("\n  _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name});").as_str());
                    }
                    if !reg_field.get_had_explicit_default() {
                        setter_interior.push(Line(format!(
                            "self.builder.set_data_field::<u16>({offset}, value as u16);"
                        )));
                    } else {
                        match reg_field.get_default_value()?.which()? {
                            schema_capnp::value::Enum(d) => {
                                setter_interior.push(Line(format!(
                                    "self.builder.set_data_field_mask::<u16>({offset}, value as u16, {d});"
                                )));
                            }
                            _ => return Err(Error::failed("enum default not an Enum".to_string())),
                        }
                    };
                    (Some(the_mod), None)
                }
                type_::Struct(st) => {
                    return_result = true;
                    initter_interior.push(
                      Line(fmt!(ctx,"{capnp}::traits::FromPointerBuilder::init_pointer(self.builder.get_pointer_field({offset}), 0)")));

                    let type_string = get_params_struct_path_string(ctx, st)?;
                    let mut lifetime = "";
                    let node::Struct(struct_node) = ctx.node_map[&st.get_type_id()].which()? else {
                        return Err(capnp::Error::failed("Type mismatch".to_string()));
                    };
                    check_fields_of_struct_for_lifetimes(
                        ctx,
                        struct_node.get_fields()?,
                        &mut lifetime,
                        0,
                    )?;
                    let the_mod = &ctx.get_qualified_module(st.get_type_id());
                    let maybe_generics = do_branding(
                        ctx,
                        st.get_type_id(),
                        st.get_brand()?,
                        Leaf::Reader(""),
                        the_mod,
                    )?;
                    let maybe_generics =
                        &maybe_generics[the_mod.len() + 9..maybe_generics.len() - 1];
                    if !lifetime.is_empty() {
                        params_struct_generics.insert("'a".to_string());
                        lifetime = "'a";
                    }
                    let bracketed_params = if !lifetime.is_empty() || maybe_generics.len() > 1 {
                        format! {"<{lifetime}{maybe_generics}>", }
                    } else {
                        "".to_string()
                    };
                    if no_discriminant && !field.has_annotations() {
                        set_types.push_str(
                            format!(", _{styled_name}: Option<{type_string}{bracketed_params}>")
                                .as_str(),
                        );
                        set_inner.push_str(format!("\n  if let Some(st) = _{styled_name} {{st.build_capnp_struct(self.reborrow().init_{styled_name}());}}").as_str());
                        result.push(Line(fmt!(ctx, "pub fn set_{styled_name}_from_struct(&mut self, st: {type_string}{bracketed_params}) {{\n      st.build_capnp_struct({capnp}::traits::FromPointerBuilder::init_pointer(self.reborrow().builder.get_pointer_field({offset}), 0));\n    }}")));

                        if type_string
                            .rfind(snake_to_camel_case(node_name).as_str())
                            .is_some()
                        {
                            rust_struct_inner.push_str(
                                format!(
                                    "{params_struct_prefix}_{styled_name}: Option<Box<{type_string}{bracketed_params}>>,"
                                )
                                .as_str(),
                            );
                            rust_struct_impl_inner.push_str(format!("\n  if let Some(st) = {params_struct_impl_prefix}_{styled_name} {{st.build_capnp_struct(_builder.reborrow().init_{styled_name}());}}").as_str());
                        } else {
                            rust_struct_inner.push_str(
                                format!(
                                    "{params_struct_prefix}_{styled_name}: Option<{type_string}{bracketed_params}>,"
                                )
                                .as_str(),
                            );
                            rust_struct_impl_inner.push_str(format!("\n  if let Some(st) = {params_struct_impl_prefix}_{styled_name} {{st.build_capnp_struct(_builder.reborrow().init_{styled_name}());}}").as_str());
                        }
                    }

                    if typ.is_branded()? {
                        setter_interior.push(
                            Line(fmt!(ctx,
                                "<{} as {capnp}::traits::SetPointerBuilder>::set_pointer_builder(self.builder.reborrow().get_pointer_field({}), value, false)",
                                typ.type_string(ctx, Leaf::Reader("'_"))?,
                                offset)));
                        (
                            Some(typ.type_string(ctx, Leaf::Reader("'_"))?),
                            Some(typ.type_string(ctx, Leaf::Builder("'a"))?),
                        )
                    } else {
                        setter_interior.push(
                            Line(fmt!(ctx,"{capnp}::traits::SetPointerBuilder::set_pointer_builder(self.builder.reborrow().get_pointer_field({offset}), value, false)")));
                        (
                            Some(reg_field.get_type()?.type_string(ctx, Leaf::Reader("'_"))?),
                            Some(
                                reg_field
                                    .get_type()?
                                    .type_string(ctx, Leaf::Builder("'a"))?,
                            ),
                        )
                    }
                }
                type_::Interface(_) => {
                    if no_discriminant {
                        set_types.push_str(
                            format!(", _{styled_name}: {}", typ.type_string(ctx, Leaf::Client)?)
                                .as_str(),
                        );
                        set_inner.push_str(
                            format!("\n  self.set_{styled_name}(_{styled_name});").as_str(),
                        );
                        rust_struct_inner.push_str(
                            format!(
                                "{params_struct_prefix}_{styled_name}: {},",
                                typ.type_string(ctx, Leaf::Client)?
                            )
                            .as_str(),
                        );
                        rust_struct_impl_inner.push_str(fmt!(ctx, "\n  _builder.set_{styled_name}({capnp}::capability::FromClientHook::new({params_struct_impl_prefix}_{styled_name}.client.hook));").as_str());
                    }
                    setter_interior.push(Line(format!(
                        "self.builder.reborrow().get_pointer_field({offset}).set_capability(value.client.hook);"
                    )));
                    (Some(typ.type_string(ctx, Leaf::Client)?), None)
                }
                type_::AnyPointer(_) => {
                    if typ.is_parameter()? {
                        let mut implicit = false;
                        for par in &used_params {
                            if interface_implicit_generics.contains(par) {
                                implicit = true;
                            }
                        }
                        params_struct_generics.insert("'a".to_string());
                        let reader_type = typ.type_string(ctx, Leaf::Reader("'a"))?;
                        if no_discriminant {
                            set_types.push_str(format!(", _{styled_name}: {reader_type}").as_str());
                            set_inner.push_str(
                                format!("\n  self.set_{styled_name}(_{styled_name}).unwrap();")
                                    .as_str(),
                            );
                            rust_struct_inner.push_str(
                                format!("{params_struct_prefix}_{styled_name}: {reader_type},")
                                    .as_str(),
                            );
                            if is_params_struct && !implicit {
                                rust_struct_impl_inner.push_str(format!("\n      _builder.reborrow().init_{styled_name}().set_as({params_struct_impl_prefix}_{styled_name}).unwrap();").as_str());
                            //TODO figure out why set_as can fail
                            } else {
                                rust_struct_impl_inner.push_str(format!("\n      _builder.set_{styled_name}({params_struct_impl_prefix}_{styled_name}).unwrap();").as_str());
                            }
                        }

                        initter_interior.push(Line(fmt!(ctx,"{capnp}::any_pointer::Builder::new(self.builder.get_pointer_field({offset})).init_as()")));
                        setter_interior.push(Line(fmt!(ctx,"{capnp}::traits::SetPointerBuilder::set_pointer_builder(self.builder.reborrow().get_pointer_field({offset}), value, false)")));
                        return_result = true;

                        let builder_type = typ.type_string(ctx, Leaf::Builder("'a"))?;

                        result.push(line("#[inline]"));
                        result.push(Line(format!(
                            "pub fn initn_{styled_name}(self, length: u32) -> {builder_type} {{"
                        )));
                        result.push(indent(initn_interior));
                        result.push(indent(
                            Line(fmt!(ctx,"{capnp}::any_pointer::Builder::new(self.builder.get_pointer_field({offset})).initn_as(length)")))
                        );
                        result.push(line("}"));

                        (
                            Some(typ.type_string(ctx, Leaf::Reader("'_"))?),
                            Some(builder_type),
                        )
                    } else {
                        //TODO
                        if no_discriminant {
                            rust_struct_inner.push_str(fmt!(ctx, "{params_struct_prefix}_{styled_name}: Box<dyn {capnp}::private::capability::ClientHook>,").as_str());
                            rust_struct_impl_inner.push_str(format!("\n  _builder.reborrow().init_{styled_name}().set_as_capability({params_struct_impl_prefix}_{styled_name});").as_str());
                        }
                        initter_interior.push(Line(fmt!(ctx,"let mut result = {capnp}::any_pointer::Builder::new(self.builder.get_pointer_field({offset}));")));
                        initter_interior.push(line("result.clear();"));
                        initter_interior.push(line("result"));
                        (None, Some(fmt!(ctx, "{capnp}::any_pointer::Builder<'a>")))
                    }
                }
                _ => return Err(Error::failed("unrecognized type".to_string())),
            }
        }
    };
    if let Some(reader_type) = &maybe_reader_type {
        let return_type = if return_result {
            fmt!(ctx, "-> {capnp}::Result<()>")
        } else {
            "".into()
        };
        result.push(line("#[inline]"));
        result.push(Line(format!(
            "pub fn set_{styled_name}(&mut self, {setter_param}: {reader_type}) {return_type} {{"
        )));
        result.push(indent(setter_interior));
        result.push(line("}"));
    }
    if let Some(builder_type) = maybe_builder_type {
        result.push(line("#[inline]"));
        let args = initter_params.join(", ");
        let mutable = if initter_mut { "mut " } else { "" };
        result.push(Line(format!(
            "pub fn init_{styled_name}({mutable}self, {args}) -> {builder_type} {{"
        )));
        result.push(indent(initter_interior));
        result.push(line("}"));
    }
    Ok(Branch(result))
}
fn check_fields_of_struct_for_lifetimes(
    ctx: &GeneratorContext,
    fields: capnp::struct_list::Reader<'_, schema_capnp::field::Owned>,
    lifetime: &mut &str,
    mut maybe_cyclical_counter: usize,
) -> ::capnp::Result<()> {
    for field in fields {
        match field.which()? {
            capnp::schema_capnp::field::Which::Slot(slot) => match slot.get_type()?.which()? {
                type_::Which::Text(_) => {
                    *lifetime = "'a, ";
                    return Ok(());
                }
                type_::Which::Data(_) => {
                    *lifetime = "'a, ";
                    return Ok(());
                }
                type_::Which::List(mut l) => loop {
                    match l.get_element_type()?.which()? {
                        type_::Which::Text(_) => {
                            *lifetime = "'a, ";
                            return Ok(());
                        }
                        type_::Which::Data(_) => {
                            *lifetime = "'a, ";
                            return Ok(());
                        }
                        type_::Which::Struct(inner) => {
                            let capnp::schema_capnp::node::Struct(struct_node) =
                                ctx.node_map[&inner.get_type_id()].which()?
                            else {
                                return Err(capnp::Error::failed("Type mismatch".to_string()));
                            };
                            if !get_params(ctx, inner.get_type_id())?.is_empty() {
                                *lifetime = "'a, ";
                                return Ok(());
                            }
                            if maybe_cyclical_counter > 10 {
                                break;
                            }
                            maybe_cyclical_counter += 1;
                            check_fields_of_struct_for_lifetimes(
                                ctx,
                                struct_node.get_fields()?,
                                lifetime,
                                maybe_cyclical_counter,
                            )?;
                        }
                        type_::Which::AnyPointer(_) => {
                            if slot.get_type()?.is_parameter()? {
                                *lifetime = "'a, ";
                                return Ok(());
                            }
                        }
                        type_::Which::List(inner) => {
                            l = inner;
                            continue;
                        }
                        _ => {
                            break;
                        }
                    }
                },
                type_::Which::Struct(inner) => {
                    let capnp::schema_capnp::node::Struct(struct_node) =
                        ctx.node_map[&inner.get_type_id()].which()?
                    else {
                        return Err(capnp::Error::failed("Type mismatch".to_string()));
                    };
                    if !get_params(ctx, inner.get_type_id())?.is_empty() {
                        *lifetime = "'a, ";
                        return Ok(());
                    }
                    if maybe_cyclical_counter > 10 {
                        continue;
                    }
                    maybe_cyclical_counter += 1;
                    check_fields_of_struct_for_lifetimes(
                        ctx,
                        struct_node.get_fields()?,
                        lifetime,
                        maybe_cyclical_counter,
                    )?;
                }
                type_::Which::Interface(_) => (),
                type_::Which::AnyPointer(_) => {
                    if slot.get_type()?.is_parameter()? {
                        *lifetime = "'a, ";
                        return Ok(());
                    }
                }
                _ => (),
            },
            capnp::schema_capnp::field::Which::Group(group) => {
                if field.get_discriminant_value() == schema_capnp::field::NO_DISCRIMINANT {
                    let capnp::schema_capnp::node::Struct(struct_node) =
                        ctx.node_map[&group.get_type_id()].which()?
                    else {
                        return Err(capnp::Error::failed("Type mismatch".to_string()));
                    };
                    if maybe_cyclical_counter > 10 {
                        continue;
                    }
                    maybe_cyclical_counter += 1;
                    check_fields_of_struct_for_lifetimes(
                        ctx,
                        struct_node.get_fields()?,
                        lifetime,
                        maybe_cyclical_counter,
                    )?;
                }
            }
        }
    }
    Ok(())
}
fn get_params_struct_path_string(
    ctx: &GeneratorContext,
    struct_reader: capnp::schema_capnp::type_::struct_::Reader,
) -> capnp::Result<String> {
    Ok(format!(
        "{}::{}",
        ctx.get_qualified_module(struct_reader.get_type_id()),
        snake_to_camel_case(ctx.get_last_name(struct_reader.get_type_id())?)
    ))
}
fn vec_of_list_element_types(
    ctx: &GeneratorContext,
    list: type_::list::Reader,
    params_struct_generics: &mut HashSet<String>,
) -> capnp::Result<String> {
    match list.get_element_type()?.which()? {
        type_::Which::Void(()) => Ok("Vec<()>".to_string()),
        type_::Which::Bool(()) => Ok("Vec<bool>".to_string()),
        type_::Which::Int8(()) => Ok("Vec<i8>".to_string()),
        type_::Which::Int16(()) => Ok("Vec<i16>".to_string()),
        type_::Which::Int32(()) => Ok("Vec<i32>".to_string()),
        type_::Which::Int64(()) => Ok("Vec<i64>".to_string()),
        type_::Which::Uint8(()) => Ok("Vec<u8>".to_string()),
        type_::Which::Uint16(()) => Ok("Vec<u16>".to_string()),
        type_::Which::Uint32(()) => Ok("Vec<u32>".to_string()),
        type_::Which::Uint64(()) => Ok("Vec<u64>".to_string()),
        type_::Which::Float32(()) => Ok("Vec<f32>".to_string()),
        type_::Which::Float64(()) => Ok("Vec<f64>".to_string()),
        type_::Which::Text(_) => {
            params_struct_generics.insert("'a".to_string());
            Ok("Vec<&'a str>".to_string())
        }
        type_::Which::Data(_) => {
            params_struct_generics.insert("'a".to_string());
            Ok("Vec<&'a [u8]>".to_string())
        }
        type_::Which::List(l) => Ok(format!(
            "Vec<{}>",
            vec_of_list_element_types(ctx, l, params_struct_generics)?
        )),
        type_::Which::Enum(enum_type) => {
            let enum_mod = ctx.get_qualified_module(enum_type.get_type_id());
            Ok(format!("Vec<{enum_mod}>"))
        }
        type_::Which::Struct(st) => {
            let capnp::schema_capnp::node::Struct(struct_node) =
                ctx.node_map[&st.get_type_id()].which()?
            else {
                return Err(capnp::Error::failed("Type mismatch".to_string()));
            };
            let mut temp = "";
            check_fields_of_struct_for_lifetimes(ctx, struct_node.get_fields()?, &mut temp, 0)?;
            if !temp.is_empty() {
                params_struct_generics.insert("'a".to_string());
                temp = "'a";
            }
            let the_mod = &ctx.get_qualified_module(st.get_type_id());
            let maybe_generics = do_branding(
                ctx,
                st.get_type_id(),
                st.get_brand()?,
                Leaf::Reader(""),
                the_mod,
            )?;
            let maybe_generics = &maybe_generics[the_mod.len() + 9..maybe_generics.len() - 1];
            let bracketed_params = if !temp.is_empty() || maybe_generics.len() > 1 {
                format! {"<{temp}{maybe_generics}>", }
            } else {
                "".to_string()
            };
            Ok(format!(
                "Vec<{}{bracketed_params}>",
                get_params_struct_path_string(ctx, st)?
            ))
        }
        type_::Which::Interface(i_t) => {
            let the_mod = ctx.get_qualified_module(i_t.get_type_id());
            let p = get_params(ctx, i_t.get_type_id())?;
            let bracketed = if p.is_empty() {
                "".to_string()
            } else {
                format!("<{}>", p.join(","))
            };
            Ok(format!("Vec<{the_mod}::Client{bracketed}>"))
        }
        type_::Which::AnyPointer(an) => {
            match an.which()? {
                type_::any_pointer::Which::Unconstrained(_) => {
                    //TODO
                    Ok(fmt!(
                        ctx,
                        "Vec<Box<dyn {capnp}::private::capability::ClientHook>>"
                    ))
                }
                type_::any_pointer::Which::Parameter(p) => {
                    params_struct_generics.insert("'a".to_string());
                    let the_struct = &ctx.node_map[&p.get_scope_id()];
                    let parameters = the_struct.get_parameters()?;
                    let parameter = parameters.get(u32::from(p.get_parameter_index()));
                    let parameter_name = parameter.get_name()?.to_str()?;
                    Ok(fmt!(
                        ctx,
                        "Vec<<{parameter_name} as {capnp}::traits::Owned>::Reader<'a>>"
                    ))
                }
                type_::any_pointer::Which::ImplicitMethodParameter(_) => Err(Error::failed(
                    "Can't be an implicit method parameter".to_string(),
                )),
            }
        }
    }
}
fn build_impl_for_list_type(
    name: &str,
    builder_variable: &str,
    list: type_::list::Reader,
    union: bool,
    is_params_struct: bool,
) -> capnp::Result<String> {
    let vec_source: String;
    if union {
        vec_source = "t".to_string();
    } else if is_params_struct {
        vec_source = format!("_{name}");
    } else {
        vec_source = format!("self._{name}");
    }
    Ok(match list.reborrow().get_element_type()?.which()? {
        type_::Which::Text(_) => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    list_builder.reborrow().set(i as u32, item.into());
                }}
            }}"
            )
        }
        type_::Which::Data(_) => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    list_builder.reborrow().set(i as u32, item);
                }}
        }}"
            )
        }
        type_::Which::List(_) => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    {}
                }}
            }}",
                build_list_of_list_impl(list.reborrow())?
            )
        }
        type_::Which::Struct(_) => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    item.build_capnp_struct(list_builder.reborrow().get(i as u32));
                }}
            }}"
            )
        }
        type_::Which::Interface(_) => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    list_builder.reborrow().set(i as u32, item.client.hook);
                }}
            }}"
            )
        }
        type_::Which::AnyPointer(_) => {
            //TODO maybe this just works, but not sure(set_as, set_as_capability)
            format!(
                        "
                    \nif !{vec_source}.is_empty() {{
                        let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                        for (i, item) in {vec_source}.into_iter().enumerate() {{
                            list_builder.reborrow().set(i as u32, item);
                        }}
                    }}"
                    )
        }
        _ => {
            format!(
                "
            \nif !{vec_source}.is_empty() {{
                let mut list_builder = {builder_variable}.reborrow().init_{name}({vec_source}.len() as u32);
                for (i, item) in {vec_source}.into_iter().enumerate() {{
                    list_builder.reborrow().set(i as u32, item);
                }}
            }}"
            )
        }
    })
}
fn build_list_of_list_impl(list: type_::list::Reader) -> capnp::Result<String> {
    Ok(match list.reborrow().get_element_type()?.which()? {
        type_::Which::Text(_) => {
            "\nlist_builder.reborrow().set(i as u32, item.into());".to_string()
        }
        type_::Which::Data(_) => "\nlist_builder.reborrow().set(i as u32, item);".to_string(),
        type_::Which::List(reader) => {
            format!("\n
                if !item.is_empty() {{
                    let mut list_builder = list_builder.reborrow().init(i as u32, item.len() as u32);
                    for (i, item) in item.into_iter().enumerate() {{ {} }}
                }}",
                build_list_of_list_impl(reader)?)
        }
        type_::Which::Struct(_) => {
            "\nitem.build_capnp_struct(list_builder.reborrow().get(i as u32));".to_string()
        }
        type_::Which::Interface(_) => {
            "\nlist_builder.reborrow().set(i as u32, item.client.hook);".to_string()
        }
        type_::Which::AnyPointer(_) => "".to_string(),
        _ => "\nlist_builder.reborrow().set(i as u32, item);".to_string(),
    })
}
fn used_params_of_group(
    ctx: &GeneratorContext,
    group_id: u64,
    used_params: &mut HashSet<String>,
) -> capnp::Result<()> {
    let node = ctx.node_map[&group_id];
    match node.which()? {
        schema_capnp::node::Struct(st) => {
            for field in st.get_fields()? {
                match field.which()? {
                    schema_capnp::field::Group(group) => {
                        used_params_of_group(ctx, group.get_type_id(), used_params)?;
                    }
                    schema_capnp::field::Slot(slot) => {
                        used_params_of_type(ctx, slot.get_type()?, used_params)?;
                    }
                }
            }
            Ok(())
        }
        _ => Err(Error::failed("not a group".to_string())),
    }
}

fn used_params_of_type(
    ctx: &GeneratorContext,
    ty: schema_capnp::type_::Reader,
    used_params: &mut HashSet<String>,
) -> capnp::Result<()> {
    match ty.which()? {
        type_::List(ls) => {
            let et = ls.get_element_type()?;
            used_params_of_type(ctx, et, used_params)?;
        }
        type_::Enum(e) => {
            let node_id = e.get_type_id();
            let brand = e.get_brand()?;
            used_params_of_brand(ctx, node_id, brand, used_params)?;
        }
        type_::Struct(s) => {
            let node_id = s.get_type_id();
            let brand = s.get_brand()?;
            used_params_of_brand(ctx, node_id, brand, used_params)?;
        }
        type_::Interface(i) => {
            let node_id = i.get_type_id();
            let brand = i.get_brand()?;
            used_params_of_brand(ctx, node_id, brand, used_params)?;
        }

        type_::AnyPointer(ap) => {
            if let type_::any_pointer::Parameter(def) = ap.which()? {
                let the_struct = &ctx.node_map[&def.get_scope_id()];
                let parameters = the_struct.get_parameters()?;
                let parameter = parameters.get(u32::from(def.get_parameter_index()));
                let parameter_name = parameter.get_name()?.to_str()?;
                used_params.insert(parameter_name.to_string());
            }
        }
        _ => (),
    }
    Ok(())
}

fn used_params_of_brand(
    ctx: &GeneratorContext,
    node_id: u64,
    brand: schema_capnp::brand::Reader,
    used_params: &mut HashSet<String>,
) -> capnp::Result<()> {
    use schema_capnp::brand;
    let scopes = brand.get_scopes()?;
    let mut brand_scopes = HashMap::new();
    for scope in scopes {
        brand_scopes.insert(scope.get_scope_id(), scope);
    }
    let brand_scopes = brand_scopes; // freeze
    let mut current_node_id = node_id;
    loop {
        let Some(current_node) = ctx.node_map.get(&current_node_id) else {
            break;
        };
        let params = current_node.get_parameters()?;
        match brand_scopes.get(&current_node_id) {
            None => (),
            Some(scope) => match scope.which()? {
                brand::scope::Inherit(()) => {
                    for param in params {
                        used_params.insert(param.get_name()?.to_string()?);
                    }
                }
                brand::scope::Bind(bindings_list_opt) => {
                    let bindings_list = bindings_list_opt?;
                    assert_eq!(bindings_list.len(), params.len());
                    for binding in bindings_list {
                        match binding.which()? {
                            brand::binding::Unbound(()) => (),
                            brand::binding::Type(t) => {
                                used_params_of_type(ctx, t?, used_params)?;
                            }
                        }
                    }
                }
            },
        }
        current_node_id = current_node.get_scope_id();
    }
    Ok(())
}

// return (the 'Which' enum, the 'which()' accessor, typedef, default_decls)
#[allow(clippy::too_many_arguments)]
fn generate_union(
    ctx: &GeneratorContext,
    discriminant_offset: u32,
    fields: &[schema_capnp::field::Reader],
    is_reader: bool,
    params: &TypeParameterTexts,
    params_struct_string: &mut String,
    params_struct_impl_string: &mut String,
    params_enum_string: &mut String,
    set_types: &mut String,
    set_inner: &mut String,
    generate_params: bool,
    union_only_struct: bool,
    params_union_name: &String,
    union_params: &mut HashSet<String>,
    union_lifetime: &mut &str,
) -> ::capnp::Result<(
    FormattedText,
    FormattedText,
    FormattedText,
    Vec<FormattedText>,
)> {
    use capnp::schema_capnp::*;

    fn new_ty_param(ty_params: &mut Vec<String>) -> String {
        let result = format!("A{}", ty_params.len());
        ty_params.push(result.clone());
        result
    }

    let mut getter_interior = Vec::new();
    let mut interior = Vec::new();
    let mut enum_interior = Vec::new();
    let mut default_decls = Vec::new();

    let mut ty_params = Vec::new();
    let mut ty_args = Vec::new();

    let mut used_params: HashSet<String> = HashSet::new();

    let doffset = discriminant_offset as usize;

    let mut params_impl_interior = String::new();
    if generate_params {
        if union_only_struct {
            params_impl_interior.push_str("\n match self {");
        } else {
            params_impl_interior.push_str("\n match self.uni {");
        }
        params_impl_interior
            .push_str(format!("\n {params_union_name}::UNINITIALIZED => (),").as_str());
        set_inner.push_str(
            format!("\n match uni {{ \n {params_union_name}::UNINITIALIZED => (),").as_str(),
        );
    }

    for field in fields {
        let dvalue = field.get_discriminant_value() as usize;

        let field_name = get_field_name(*field)?;
        let enumerant_name = capitalize_first_letter(field_name);

        if generate_params {
            let camel = camel_to_snake_case(field_name);
            match field.which()? {
                field::Which::Slot(reg_field) => {
                    let typ = reg_field.reborrow().get_type()?;
                    let mut used_params = HashSet::new();
                    used_params_of_type(ctx, typ.reborrow(), &mut used_params)?;
                    for par in &used_params {
                        union_params.insert(par.to_string());
                    }
                    match reg_field.get_type()?.which()? {
                        type_::Which::Text(_) => {
                            *union_lifetime = "'a,";
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(&'a str),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.reborrow().set_{}(t.into()),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t.into()),", camel.as_str()).as_str());
                        }
                        type_::Which::Data(_) => {
                            *union_lifetime = "'a,";
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(&'a [u8]),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.reborrow().set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::List(l) => {
                            let mut temp = HashSet::new();
                            if let Ok(vec_of_list_element_types) =
                                vec_of_list_element_types(ctx, l.reborrow(), &mut temp)
                            {
                                params_enum_string.push_str(
                                    format!("\n _{enumerant_name}({vec_of_list_element_types}),",)
                                        .as_str(),
                                );
                                params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => {{\n{}\n}},", build_impl_for_list_type(camel.as_str(), "_builder", l.reborrow(), true, false)?).as_str());
                                set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => {{\n{}\n}}", build_impl_for_list_type(camel.as_str(), "self", l.reborrow(), true, false)?).as_str());
                            }
                            if !temp.is_empty() {
                                *union_lifetime = "'a,";
                            }
                        }
                        type_::Which::Enum(e) => {
                            let id = e.get_type_id();
                            let the_mod = ctx.get_qualified_module(id);
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}({the_mod}),",).as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.reborrow().set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Struct(st) => {
                            let path_string = get_params_struct_path_string(ctx, st)?;
                            let mut possibly_cyclical = false;
                            let mut temp = "";
                            let schema_capnp::node::Which::Struct(struct_node) =
                                ctx.node_map[&st.get_type_id()].which()?
                            else {
                                return Err(capnp::Error::failed("Type mismatch".to_string()));
                            };
                            check_fields_of_struct_for_lifetimes(
                                ctx,
                                struct_node.get_fields()?,
                                &mut temp,
                                0,
                            )?;
                            if !temp.is_empty() {
                                *union_lifetime = "'a,";
                                temp = "'a";
                            }
                            for field in struct_node.get_fields()? {
                                match field.which()? {
                                    field::Which::Slot(sl) => match sl.get_type()?.which()? {
                                        type_::Which::List(_) => possibly_cyclical = true,
                                        type_::Which::Struct(_) => possibly_cyclical = true,
                                        _ => (),
                                    },
                                    field::Which::Group(_) => possibly_cyclical = true,
                                }
                            }
                            let the_mod = &ctx.get_qualified_module(st.get_type_id());
                            let maybe_generics = do_branding(
                                ctx,
                                st.get_type_id(),
                                st.get_brand()?,
                                Leaf::Reader(""),
                                the_mod,
                            )?;
                            let maybe_generics =
                                &maybe_generics[the_mod.len() + 9..maybe_generics.len() - 1];
                            let bracketed_params = if !temp.is_empty() || maybe_generics.len() > 1 {
                                format! {"<{temp}{maybe_generics}>", }
                            } else {
                                "".to_string()
                            };
                            if possibly_cyclical {
                                params_enum_string.push_str(
                                    format!(
                                        "\n _{enumerant_name}(Box<{path_string}{bracketed_params}>),",
                                    )
                                    .as_str(),
                                );
                                params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => t.build_capnp_struct(_builder.reborrow().init_{}()),", camel.as_str()).as_str());
                            } else {
                                params_enum_string.push_str(
                                    format!(
                                        "\n _{enumerant_name}({path_string}{bracketed_params}),",
                                    )
                                    .as_str(),
                                );
                                params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => t.build_capnp_struct(_builder.reborrow().init_{}()),", camel.as_str()).as_str());
                            }
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => t.build_capnp_struct(self.reborrow().init_{}()),", camel.as_str()).as_str());
                        }
                        type_::Which::Interface(_) => {
                            params_enum_string.push_str(
                                format!(
                                    "\n _{enumerant_name}({}),",
                                    reg_field.get_type()?.type_string(ctx, Leaf::Client)?
                                )
                                .as_str(),
                            );
                            params_impl_interior.push_str(format!("\n  {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::AnyPointer(an) => {
                            match an.which()? {
                                type_::any_pointer::Which::Unconstrained(_) => {
                                    //TODO
                                    params_enum_string.push_str(fmt!(ctx, "\n _{enumerant_name}(Box<dyn {capnp}::private::capability::ClientHook>),").as_str());
                                    params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.reborrow().init_{}().set_as_capability(t),", camel.as_str()).as_str());
                                }
                                type_::any_pointer::Which::ImplicitMethodParameter(_) => (),
                                type_::any_pointer::Which::Parameter(_) => {
                                    *union_lifetime = "'a,";
                                    let reader_type = typ.type_string(ctx, Leaf::Reader("'a"))?;
                                    params_enum_string.push_str(
                                        format!("\n _{enumerant_name}({reader_type}),").as_str(),
                                    );
                                    params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t).unwrap(),", camel.as_str()).as_str());
                                }
                            }
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t).unwrap(),", camel.as_str()).as_str());
                        }
                        type_::Which::Void(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(()),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Bool(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(bool),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Int8(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(i8),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Int16(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(i16),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Int32(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(i32),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Int64(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(i64),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Uint8(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(u8),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Uint16(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(u16),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.reborrow().set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Uint32(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(u32),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Uint64(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(u64),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Float32(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(f32),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                        type_::Which::Float64(_) => {
                            params_enum_string
                                .push_str(format!("\n _{enumerant_name}(f64),").as_str());
                            params_impl_interior.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => _builder.set_{}(t),", camel.as_str()).as_str());
                            set_inner.push_str(format!("\n {params_union_name}::_{enumerant_name}(t) => self.set_{}(t),", camel.as_str()).as_str());
                        }
                    }
                }
                field::Which::Group(_) => (),
            }
        }

        let (ty, get, maybe_default_decl) = getter_text(ctx, field, is_reader, false)?;
        if let Some(default_decl) = maybe_default_decl {
            default_decls.push(default_decl)
        }

        getter_interior.push(Branch(vec![
            Line(format!("{dvalue} => {{")),
            indent(Line(format!(
                "::core::result::Result::Ok({}(",
                enumerant_name.clone()
            ))),
            indent(indent(get)),
            indent(line("))")),
            line("}"),
        ]));

        let ty1 = match field.which() {
            Ok(field::Group(group)) => {
                used_params_of_group(ctx, group.get_type_id(), &mut used_params)?;
                ty_args.push(ty);
                new_ty_param(&mut ty_params)
            }
            Ok(field::Slot(reg_field)) => {
                let fty = reg_field.get_type()?;
                used_params_of_type(ctx, fty, &mut used_params)?;
                match fty.which() {
                    Ok(
                        type_::Text(())
                        | type_::Data(())
                        | type_::List(_)
                        | type_::Struct(_)
                        | type_::AnyPointer(_),
                    ) => {
                        ty_args.push(ty);
                        new_ty_param(&mut ty_params)
                    }
                    Ok(type_::Interface(_)) => ty,
                    _ => ty,
                }
            }
            _ => ty,
        };

        enum_interior.push(Line(format!("{enumerant_name}({ty1}),")));
    }
    set_inner.push_str("\n }");
    let enum_name = format!(
        "Which{}",
        if !ty_params.is_empty() {
            format!("<{}>", ty_params.join(","))
        } else {
            "".to_string()
        }
    );
    let bracketed = if !union_params.is_empty() || !union_lifetime.is_empty() {
        format!(
            "<{union_lifetime}{}>",
            union_params
                .clone()
                .into_iter()
                .collect::<Vec<String>>()
                .join(",")
        )
    } else {
        "".to_string()
    };
    if generate_params {
        if !union_only_struct {
            params_struct_string
                .push_str(format!("\n pub uni: {params_union_name}{bracketed},").as_str());
            set_types.push_str(format!(", uni: {params_union_name}{bracketed}").as_str());
        }
        if !params_impl_interior.is_empty() {
            params_struct_impl_string.push_str(format!("{params_impl_interior}\n }}").as_str());
        }
    }

    getter_interior.push(Line(fmt!(
        ctx,
        "x => ::core::result::Result::Err({capnp}::NotInSchema(x))"
    )));

    interior.push(Branch(vec![
        Line(format!("pub enum {enum_name} {{")),
        indent(enum_interior),
        line("}"),
    ]));

    let result = Branch(interior);

    let field_name = if is_reader { "reader" } else { "builder" };

    let concrete_type = format!(
        "Which{}{}",
        if is_reader { "Reader" } else { "Builder" },
        if !ty_params.is_empty() {
            format!(
                "<'a,{}>",
                params
                    .expanded_list
                    .iter()
                    .filter(|s: &&String| used_params.contains(*s))
                    .cloned()
                    .collect::<Vec<String>>()
                    .join(",")
            )
        } else {
            "".to_string()
        }
    );

    let typedef = Line(format!(
        "pub type {concrete_type} = Which{};",
        if !ty_args.is_empty() {
            format!("<{}>", ty_args.join(","))
        } else {
            "".to_string()
        }
    ));

    let getter_result = Branch(vec![
        line("#[inline]"),
        Line(fmt!(
            ctx,
            "pub fn which(self) -> ::core::result::Result<{concrete_type}, {capnp}::NotInSchema> {{"
        )),
        indent(vec![
            Line(format!(
                "match self.{field_name}.get_data_field::<u16>({doffset}) {{"
            )),
            indent(getter_interior),
            line("}"),
        ]),
        line("}"),
    ]);

    // TODO set_which() for builders?

    Ok((result, getter_result, typedef, default_decls))
}

fn generate_haser(
    discriminant_offset: u32,
    styled_name: &str,
    field: &schema_capnp::field::Reader,
    is_reader: bool,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::*;

    let mut result = Vec::new();
    let mut interior = Vec::new();
    let member = if is_reader { "reader" } else { "builder" };

    let discriminant_value = field.get_discriminant_value();
    if discriminant_value != field::NO_DISCRIMINANT {
        interior.push(Line(format!(
            "if self.{}.get_data_field::<u16>({}) != {} {{ return false; }}",
            member, discriminant_offset as usize, discriminant_value as usize
        )));
    }
    match field.which() {
        Err(_) | Ok(field::Group(_)) => {}
        Ok(field::Slot(reg_field)) => match reg_field.get_type()?.which()? {
            type_::Text(())
            | type_::Data(())
            | type_::List(_)
            | type_::Struct(_)
            | type_::Interface(_)
            | type_::AnyPointer(_) => {
                if is_reader {
                    interior.push(Line(format!(
                        "!self.{member}.get_pointer_field({}).is_null()",
                        reg_field.get_offset()
                    )));
                } else {
                    interior.push(Line(format!(
                        "!self.{member}.is_pointer_field_null({})",
                        reg_field.get_offset()
                    )));
                }
                result.push(line("#[inline]"));
                result.push(Line(format!("pub fn has_{styled_name}(&self) -> bool {{")));
                result.push(indent(interior));
                result.push(line("}"));
            }
            _ => {}
        },
    }

    Ok(Branch(result))
}

fn generate_pipeline_getter(
    ctx: &GeneratorContext,
    field: schema_capnp::field::Reader,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::field;

    let name = get_field_name(field)?;

    match field.which()? {
        field::Group(group) => {
            let params = get_params(ctx, group.get_type_id())?;
            let params_string = if params.is_empty() {
                "".to_string()
            } else {
                format!("<{}>", params.join(","))
            };

            let the_mod = ctx.get_qualified_module(group.get_type_id());
            Ok(Branch(vec![
                Line(format!(
                    "pub fn get_{}(&self) -> {}::Pipeline{} {{",
                    camel_to_snake_case(name),
                    the_mod,
                    params_string
                )),
                indent(Line(fmt!(
                    ctx,
                    "{capnp}::capability::FromTypelessPipeline::new(self._typeless.noop())"
                ))),
                line("}"),
            ]))
        }
        field::Slot(reg_field) => {
            let typ = reg_field.get_type()?;
            match typ.which()? {
                type_::Struct(_) | type_::AnyPointer(_) => Ok(Branch(vec![
                    Line(format!(
                        "pub fn get_{}(&self) -> {} {{",
                        camel_to_snake_case(name),
                        typ.type_string(ctx, Leaf::Pipeline)?
                    )),
                    indent(Line(fmt!(
                        ctx,
                        "{capnp}::capability::FromTypelessPipeline::new(self._typeless.get_pointer_field({}))",
                        reg_field.get_offset()
                    ))),
                    line("}"),
                ])),
                type_::Interface(_) => Ok(Branch(vec![
                    Line(format!(
                        "pub fn get_{}(&self) -> {} {{",
                        camel_to_snake_case(name),
                        typ.type_string(ctx, Leaf::Client)?
                    )),
                    indent(Line(fmt!(
                        ctx,
                        "{capnp}::capability::FromClientHook::new(self._typeless.get_pointer_field({}).as_cap())",
                        reg_field.get_offset()
                    ))),
                    line("}"),
                ])),
                _ => Ok(Branch(Vec::new())),
            }
        }
    }
}

fn generate_get_field_types(
    ctx: &GeneratorContext,
    node_reader: schema_capnp::node::Reader,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::field;
    let st = match node_reader.which()? {
        schema_capnp::node::Struct(st) => st,
        _ => return Err(Error::failed("not a struct".into())),
    };
    let mut branches = vec![];
    for (index, field) in st.get_fields()?.iter().enumerate() {
        match field.which()? {
            field::Slot(slot) => {
                let raw_type = slot.get_type()?;
                let typ = raw_type.type_string(ctx, Leaf::Owned)?;
                branches.push(Line(fmt!(
                    ctx,
                    "{} => <{} as {capnp}::introspect::Introspect>::introspect(),",
                    index,
                    typ
                )));
            }
            field::Group(group) => {
                let params = get_params(ctx, group.get_type_id())?;
                let params_string = if params.is_empty() {
                    "".to_string()
                } else {
                    format!("<{}>", params.join(","))
                };

                let the_mod = ctx.get_qualified_module(group.get_type_id());

                let typ = format!("{the_mod}::Owned{params_string}");
                branches.push(Line(fmt!(
                    ctx,
                    "{} => <{} as {capnp}::introspect::Introspect>::introspect(),",
                    index,
                    typ
                )));
            }
        }
    }
    let body = if branches.is_empty() {
        Line("panic!(\"invalid field index {index}\")".into())
    } else {
        branches.push(Line("_ => panic!(\"invalid field index {index}\"),".into()));
        Branch(vec![
            Line("match index {".into()),
            indent(branches),
            Line("}".into()),
        ])
    };
    if !node_reader.get_is_generic() {
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_field_types(index: u16) -> {capnp}::introspect::Type {{"
            )),
            indent(body),
            Line("}".into()),
        ]))
    } else {
        let params = node_reader.parameters_texts(ctx);
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_field_types<{0}>(index: u16) -> {capnp}::introspect::Type {1} {{",
                params.params,
                params.where_clause
            )),
            indent(body),
            Line("}".into()),
        ]))
    }
}

fn generate_get_params_results(
    ctx: &GeneratorContext,
    names: &Vec<String>,
    node_reader: schema_capnp::node::Reader,
) -> ::capnp::Result<FormattedText> {
    let i = match node_reader.which()? {
        schema_capnp::node::Interface(i) => i,
        _ => return Err(Error::failed("not an interface".into())),
    };
    let mut params_branches = vec![];
    let mut results_branches = vec![];
    let methods = i.get_methods()?;
    for (ordinal, method) in methods.into_iter().enumerate() {
        let name = method.get_name()?.to_str()?;
        let param_id = method.get_param_struct_type();
        let param_node = &ctx.node_map[&param_id];
        let param_scopes = if param_node.get_scope_id() == 0 {
            let mut names = names.to_owned();
            let local_name = module_name(&format!("{name}Params"));
            names.push(local_name);
            names
        } else {
            ctx.scope_map[&param_node.get_id()].clone()
        };
        let param_type = do_branding(
            ctx,
            param_id,
            method.get_param_brand()?,
            Leaf::Owned,
            &param_scopes.join("::"),
        )?;
        params_branches.push(Line(fmt!(
            ctx,
            "{} => <{} as {capnp}::introspect::Introspect>::introspect(),",
            ordinal,
            param_type
        )));

        let result_id = method.get_result_struct_type();
        let result_node = &ctx.node_map[&result_id];
        let result_scopes = if result_node.get_scope_id() == 0 {
            let mut names = names.to_owned();
            let local_name = module_name(&format!("{name}Results"));
            names.push(local_name);
            names
        } else {
            ctx.scope_map[&result_node.get_id()].clone()
        };
        let result_type = do_branding(
            ctx,
            result_id,
            method.get_result_brand()?,
            Leaf::Owned,
            &result_scopes.join("::"),
        )?;
        results_branches.push(Line(fmt!(
            ctx,
            "{} => <{} as {capnp}::introspect::Introspect>::introspect(),",
            ordinal,
            result_type
        )));
    }
    let params_body = if params_branches.is_empty() {
        Line("panic!(\"invalid field index {index}\")".into())
    } else {
        params_branches.push(Line("_ => panic!(\"invalid field index {index}\"),".into()));
        Branch(vec![
            Line("match index {".into()),
            indent(params_branches),
            Line("}".into()),
        ])
    };
    let results_body = if results_branches.is_empty() {
        Line("panic!(\"invalid field index {index}\")".into())
    } else {
        results_branches.push(Line("_ => panic!(\"invalid field index {index}\"),".into()));
        Branch(vec![
            Line("match index {".into()),
            indent(results_branches),
            Line("}".into()),
        ])
    };
    if !node_reader.get_is_generic() {
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_param_type(index: u16) -> {capnp}::introspect::Type {{"
            )),
            indent(params_body),
            Line("}".into()),
            Line(fmt!(
                ctx,
                "pub fn get_result_type(index: u16) -> {capnp}::introspect::Type {{"
            )),
            indent(results_body),
            Line("}".into()),
        ]))
    } else {
        let params = node_reader.parameters_texts(ctx);
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_param_type<{0}>(index: u16) -> {capnp}::introspect::Type {1} {{",
                params.params,
                params.where_clause
            )),
            indent(params_body),
            Line("}".into()),
            Line(fmt!(
                ctx,
                "pub fn get_result_type<{0}>(index: u16) -> {capnp}::introspect::Type {1} {{",
                params.params,
                params.where_clause
            )),
            indent(results_body),
            Line("}".into()),
        ]))
    }
}

fn annotation_branch(
    ctx: &GeneratorContext,
    annotation: schema_capnp::annotation::Reader,
    child_index: Option<u16>,
    index: u32,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::node;
    let id = annotation.get_id();
    let annotation_decl = ctx.node_map[&id];
    let node::Annotation(a) = annotation_decl.which()? else {
        return Err(Error::failed("not an annotation node".into()));
    };
    if annotation_decl.get_is_generic() {
        let brand = annotation.get_brand()?;
        let the_mod = ctx.get_qualified_module(id);
        let func = do_branding(ctx, id, brand, Leaf::GetType, &the_mod)?;
        Ok(Line(format!("({child_index:?}, {index}) => {func}(),",)))
    } else {
        // Avoid referring to the annotation in the generated code, so that users can import
        // annotation schemas like `c++.capnp` or `rust.capnp` without needing to generate code
        // for them, as long as the annotations are not generic.
        let ty = a.get_type()?;
        Ok(Line(fmt!(
            ctx,
            "({child_index:?}, {index}) => <{} as {capnp}::introspect::Introspect>::introspect(),",
            ty.type_string(ctx, Leaf::Owned)?
        )))
    }
}

fn generate_get_annotation_types(
    ctx: &GeneratorContext,
    node_reader: schema_capnp::node::Reader,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::node;

    let mut branches = vec![];

    for (idx, annotation) in node_reader.get_annotations()?.iter().enumerate() {
        branches.push(annotation_branch(ctx, annotation, None, idx as u32)?);
    }

    match node_reader.which()? {
        node::Struct(s) => {
            for (fidx, field) in s.get_fields()?.iter().enumerate() {
                for (idx, annotation) in field.get_annotations()?.iter().enumerate() {
                    branches.push(annotation_branch(
                        ctx,
                        annotation,
                        Some(fidx as u16),
                        idx as u32,
                    )?);
                }
            }
        }
        node::Enum(e) => {
            for (fidx, enumerant) in e.get_enumerants()?.iter().enumerate() {
                for (idx, annotation) in enumerant.get_annotations()?.iter().enumerate() {
                    branches.push(annotation_branch(
                        ctx,
                        annotation,
                        Some(fidx as u16),
                        idx as u32,
                    )?);
                }
            }
        }
        _ => (),
    }

    let body = if branches.is_empty() {
        Line("panic!(\"invalid annotation indices ({child_index:?}, {index}) \");".into())
    } else {
        branches.push(Line(
            "_ => panic!(\"invalid annotation indices ({child_index:?}, {index}) \"),".into(),
        ));
        indent(vec![
            Line("match (child_index, index) {".into()),
            indent(branches),
            Line("}".into()),
        ])
    };

    if !node_reader.get_is_generic() {
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_annotation_types(child_index: Option<u16>, index: u32) -> {capnp}::introspect::Type {{"
            )),
            indent(body),
            Line("}".into()),
        ]))
    } else {
        let params = node_reader.parameters_texts(ctx);
        Ok(Branch(vec![
            Line(fmt!(
                ctx,
                "pub fn get_annotation_types<{0}>(child_index: Option<u16>, index: u32) -> {capnp}::introspect::Type {1} {{",
                params.params,
                params.where_clause
            )),
            indent(body),
            Line("}".into()),
        ]))
    }
}

fn generate_members_by_discriminant(
    node_reader: schema_capnp::node::Reader,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::field;
    let st = match node_reader.which()? {
        schema_capnp::node::Struct(st) => st,
        _ => return Err(Error::failed("not a struct".into())),
    };

    let mut union_member_indexes = vec![];
    let mut nonunion_member_indexes = vec![];
    for (index, field) in st.get_fields()?.iter().enumerate() {
        let disc = field.get_discriminant_value();
        if disc == field::NO_DISCRIMINANT {
            nonunion_member_indexes.push(index);
        } else {
            union_member_indexes.push((disc, index));
        }
    }
    union_member_indexes.sort();

    let mut nonunion_string: String = "pub static NONUNION_MEMBERS : &[u16] = &[".into();
    for idx in 0..nonunion_member_indexes.len() {
        nonunion_string += &format!("{}", nonunion_member_indexes[idx]);
        if idx + 1 < nonunion_member_indexes.len() {
            nonunion_string += ",";
        }
    }
    nonunion_string += "];";

    let mut members_by_disc: String = "pub static MEMBERS_BY_DISCRIMINANT : &[u16] = &[".into();
    for idx in 0..union_member_indexes.len() {
        let (disc, index) = union_member_indexes[idx];
        assert_eq!(idx, disc as usize);
        members_by_disc += &format!("{index}");
        if idx + 1 < union_member_indexes.len() {
            members_by_disc += ",";
        }
    }
    members_by_disc += "];";
    Ok(Branch(vec![Line(nonunion_string), Line(members_by_disc)]))
}

// We need this to work around the fact that Rust does not allow typedefs
// with unused type parameters.
fn get_ty_params_of_brand(
    ctx: &GeneratorContext,
    brand: schema_capnp::brand::Reader,
) -> ::capnp::Result<String> {
    let mut acc = HashSet::new();
    get_ty_params_of_brand_helper(ctx, &mut acc, brand)?;
    let mut result = String::new();
    for (scope_id, parameter_index) in acc.into_iter() {
        let node = ctx.node_map[&scope_id];
        let p = node.get_parameters()?.get(u32::from(parameter_index));
        result.push_str(p.get_name()?.to_str()?);
        result.push(',');
    }

    Ok(result)
}

fn get_ty_params_of_type_helper(
    ctx: &GeneratorContext,
    accumulator: &mut HashSet<(u64, u16)>,
    typ: schema_capnp::type_::Reader,
) -> ::capnp::Result<()> {
    match typ.which()? {
        type_::Void(())
        | type_::Bool(())
        | type_::Int8(())
        | type_::Int16(())
        | type_::Int32(())
        | type_::Int64(())
        | type_::Uint8(())
        | type_::Uint16(())
        | type_::Uint32(())
        | type_::Uint64(())
        | type_::Float32(())
        | type_::Float64(())
        | type_::Text(_)
        | type_::Data(_) => {}
        type_::AnyPointer(p) => {
            match p.which()? {
                type_::any_pointer::Unconstrained(_) => (),
                type_::any_pointer::Parameter(p) => {
                    accumulator.insert((p.get_scope_id(), p.get_parameter_index()));
                }
                type_::any_pointer::ImplicitMethodParameter(_) => {
                    // XXX
                }
            }
        }
        type_::List(list) => {
            get_ty_params_of_type_helper(ctx, accumulator, list.get_element_type()?)?
        }
        type_::Enum(e) => {
            get_ty_params_of_brand_helper(ctx, accumulator, e.get_brand()?)?;
        }
        type_::Struct(s) => {
            get_ty_params_of_brand_helper(ctx, accumulator, s.get_brand()?)?;
        }
        type_::Interface(interf) => {
            get_ty_params_of_brand_helper(ctx, accumulator, interf.get_brand()?)?;
        }
    }
    Ok(())
}

fn get_ty_params_of_brand_helper(
    ctx: &GeneratorContext,
    accumulator: &mut HashSet<(u64, u16)>,
    brand: schema_capnp::brand::Reader,
) -> ::capnp::Result<()> {
    for scope in brand.get_scopes()? {
        let scope_id = scope.get_scope_id();
        match scope.which()? {
            schema_capnp::brand::scope::Bind(bind) => {
                for binding in bind? {
                    match binding.which()? {
                        schema_capnp::brand::binding::Unbound(()) => {}
                        schema_capnp::brand::binding::Type(t) => {
                            get_ty_params_of_type_helper(ctx, accumulator, t?)?
                        }
                    }
                }
            }
            schema_capnp::brand::scope::Inherit(()) => {
                let parameters = ctx.node_map[&scope_id].get_parameters()?;
                for idx in 0..parameters.len() {
                    accumulator.insert((scope_id, idx as u16));
                }
            }
        }
    }
    Ok(())
}

fn generate_node(
    ctx: &GeneratorContext,
    node_id: u64,
    node_name: &str,
    rust_struct_inner: &mut String,
    rust_struct_impl_inner: &mut String,
    params_struct_generics: &mut HashSet<String>,
    interface_implicit_generics: &[String],
    is_params_struct: bool,
) -> ::capnp::Result<FormattedText> {
    use capnp::schema_capnp::*;

    let mut output: Vec<FormattedText> = Vec::new();
    let mut nested_output: Vec<FormattedText> = Vec::new();

    let node_reader = &ctx.node_map[&node_id];
    let nested_nodes = node_reader.get_nested_nodes()?;
    for nested_node in nested_nodes {
        let id = nested_node.get_id();
        nested_output.push(generate_node(
            ctx,
            id,
            ctx.get_last_name(id)?,
            &mut String::new(),
            &mut String::new(),
            &mut HashSet::new(),
            &Vec::new(),
            false,
        )?);
    }

    match node_reader.which()? {
        node::File(()) => {
            output.push(Branch(nested_output));
        }
        node::Struct(struct_reader) => {
            let params = node_reader.parameters_texts(ctx);
            output.push(BlankLine);

            let is_generic = node_reader.get_is_generic();
            if is_generic {
                output.push(Line(format!(
                    "pub mod {} {{ /* {} */",
                    node_name,
                    params.expanded_list.join(",")
                )));
            } else {
                output.push(Line(format!("pub mod {node_name} {{")));
            }
            output.push(line("#![allow(clippy::extra_unused_type_parameters)]"));
            output.push(line("#![allow(clippy::needless_lifetimes)]"));

            output.push(BlankLine);
            let bracketed_params = if params.params.is_empty() {
                "".to_string()
            } else {
                format!("<{}>", params.params)
            };

            let mut preamble = Vec::new();
            let mut builder_members = Vec::new();
            let mut reader_members = Vec::new();
            let mut union_fields = Vec::new();
            let mut which_enums = Vec::new();
            let mut pipeline_impl_interior = Vec::new();
            let mut private_mod_interior = Vec::new();

            let data_size = struct_reader.get_data_word_count();
            let pointer_size = struct_reader.get_pointer_count();
            let discriminant_count = struct_reader.get_discriminant_count();
            let discriminant_offset = struct_reader.get_discriminant_offset();

            private_mod_interior.push(crate::pointer_constants::node_word_array_declaration(
                ctx,
                "ENCODED_NODE",
                *node_reader,
                crate::pointer_constants::WordArrayDeclarationOptions { public: true },
            )?);

            private_mod_interior.push(generate_get_field_types(ctx, *node_reader)?);
            private_mod_interior.push(generate_get_annotation_types(ctx, *node_reader)?);

            // `static` instead of `const` so that this has a fixed memory address
            // and we can check equality of `RawStructSchema` values by comparing pointers.
            private_mod_interior.push(Branch(vec![
                Line(fmt!(ctx,"pub static RAW_SCHEMA: {capnp}::introspect::RawStructSchema = {capnp}::introspect::RawStructSchema {{")),
                indent(vec![
                    Line("encoded_node: &ENCODED_NODE,".into()),
                    Line("nonunion_members: NONUNION_MEMBERS,".into()),
                    Line("members_by_discriminant: MEMBERS_BY_DISCRIMINANT,".into()),
                ]),
                Line("};".into()),
            ]));

            private_mod_interior.push(generate_members_by_discriminant(*node_reader)?);

            let mut params_struct_string = String::new();
            let mut params_struct_impl_string = String::new();
            let mut set_types = String::new();
            let mut set_inner = String::new();
            let mut union_only_struct = true;

            let fields = struct_reader.get_fields()?;
            for field in fields {
                let name = get_field_name(field)?;
                let styled_name = camel_to_snake_case(name);

                let discriminant_value = field.get_discriminant_value();
                let is_union_field = discriminant_value != field::NO_DISCRIMINANT;

                if !is_union_field {
                    union_only_struct = false;
                    pipeline_impl_interior.push(generate_pipeline_getter(ctx, field)?);
                    let (ty, get, default_decl) = getter_text(ctx, &field, true, true)?;
                    if let Some(default) = default_decl {
                        private_mod_interior.push(default.clone());
                    }
                    reader_members.push(Branch(vec![
                        line("#[inline]"),
                        Line(format!("pub fn get_{styled_name}(self) {ty} {{")),
                        indent(get),
                        line("}"),
                    ]));

                    let (ty_b, get_b, _) = getter_text(ctx, &field, false, true)?;
                    builder_members.push(Branch(vec![
                        line("#[inline]"),
                        Line(format!("pub fn get_{styled_name}(self) {ty_b} {{")),
                        indent(get_b),
                        line("}"),
                    ]));
                } else {
                    union_fields.push(field);
                }
                builder_members.push(generate_setter(
                    ctx,
                    discriminant_offset,
                    &styled_name,
                    &field,
                    rust_struct_inner,
                    rust_struct_impl_inner,
                    &mut set_types,
                    &mut set_inner,
                    is_params_struct,
                    params_struct_generics,
                    interface_implicit_generics,
                    node_name,
                )?);

                reader_members.push(generate_haser(
                    discriminant_offset,
                    &styled_name,
                    &field,
                    true,
                )?);
                builder_members.push(generate_haser(
                    discriminant_offset,
                    &styled_name,
                    &field,
                    false,
                )?);

                if let Ok(field::Group(group)) = field.which() {
                    let id = group.get_type_id();
                    let text = generate_node(
                        ctx,
                        id,
                        ctx.get_last_name(id)?,
                        &mut String::new(),
                        &mut String::new(),
                        &mut HashSet::new(),
                        &Vec::new(),
                        false,
                    )?;
                    nested_output.push(text);
                }
            }
            let mut implicit_generics = String::new();
            let mut bracketed = "<".to_string();
            let mut bracketed_with_where = "<".to_string();
            if params_struct_generics.remove("'a") {
                bracketed.push_str("'a,");
                bracketed_with_where.push_str("'a,");
            }
            if params.expanded_list.len() > params_struct_generics.len() {
                implicit_generics.push('<');
            }
            for param in &params.expanded_list {
                if params_struct_generics.contains(param) {
                    bracketed.push_str(param.as_str());
                    bracketed.push(',');
                    bracketed_with_where.push_str(param.as_str());
                    bracketed_with_where.push_str(fmt!(ctx, ": {capnp}::traits::Owned,").as_str());
                } else {
                    implicit_generics.push_str(param.as_str());
                    implicit_generics.push_str(fmt!(ctx, ": {capnp}::traits::Owned,").as_str());
                }
            }
            bracketed.push('>');
            bracketed_with_where.push('>');
            if params.expanded_list.len() > params_struct_generics.len() {
                implicit_generics.push('>');
            }
            if bracketed.len() == 2 {
                bracketed = "".to_string();
                bracketed_with_where = "".to_string();
            }
            params_struct_string = format!(
                "pub struct {}{bracketed_with_where} {{{params_struct_string}",
                snake_to_camel_case(node_name)
            );
            let mut params_enum_string = String::new();
            let mut union_params = HashSet::new();
            let mut union_lifetime = "";
            if discriminant_count > 0 {
                let mut params_union_name;
                if union_only_struct {
                    params_union_name = snake_to_camel_case(node_name);
                    params_struct_string = "".to_string();
                } else {
                    params_union_name = snake_to_camel_case(node_name);
                    params_union_name.push_str("Union");
                }

                let (which_enums1, union_getter, typedef, mut default_decls) = generate_union(
                    ctx,
                    discriminant_offset,
                    &union_fields,
                    true,
                    &params,
                    &mut params_struct_string,
                    rust_struct_impl_inner,
                    &mut params_enum_string,
                    &mut set_types,
                    &mut set_inner,
                    true,
                    union_only_struct,
                    &params_union_name,
                    &mut union_params,
                    &mut union_lifetime,
                )?;
                which_enums.push(which_enums1);
                which_enums.push(typedef);
                reader_members.push(union_getter);

                private_mod_interior.append(&mut default_decls);

                let (_, union_getter, typedef, _) = generate_union(
                    ctx,
                    discriminant_offset,
                    &union_fields,
                    false,
                    &params,
                    &mut params_struct_string,
                    &mut params_struct_impl_string,
                    &mut params_enum_string,
                    &mut String::new(),
                    &mut String::new(),
                    false,
                    union_only_struct,
                    &params_union_name,
                    &mut HashSet::new(),
                    &mut "",
                )?;
                which_enums.push(typedef);
                builder_members.push(union_getter);

                let mut reexports = String::new();
                reexports.push_str("pub use self::Which::{");
                let mut whichs = Vec::new();
                for f in &union_fields {
                    whichs.push(capitalize_first_letter(get_field_name(*f)?));
                }
                reexports.push_str(&whichs.join(","));
                reexports.push_str("};");
                preamble.push(Line(reexports));
                preamble.push(BlankLine);
                let enum_bracketed = if union_only_struct {
                    set_inner = String::new();
                    bracketed_with_where.clone()
                } else if !union_params.is_empty() || !union_lifetime.is_empty() {
                    let mut temp = format!("<{union_lifetime}");
                    for p in union_params.iter() {
                        temp.push_str(fmt!(ctx, "{p}: {capnp}::traits::Owned,").as_str());
                    }
                    temp.push('>');
                    temp
                } else {
                    "".to_string()
                };
                params_enum_string = format!(
                    "pub enum {params_union_name}{enum_bracketed} {{\n UNINITIALIZED,{params_enum_string}"
                );
            }

            if !params_enum_string.is_empty() {
                params_enum_string.push_str("\n}");
            }
            params_struct_impl_string = format!(
                "impl {bracketed_with_where} {}{bracketed} {{",
                snake_to_camel_case(node_name)
            );
            params_struct_impl_string.push_str(
                format!(
                    "\npub fn build_capnp_struct{}(self, mut _builder: Builder<'_,{}>) {{",
                    implicit_generics, params.params
                )
                .as_str(),
            );
            let set = if set_inner.is_empty() {
                BlankLine
            } else {
                Branch(vec![
                    Line("#[allow(clippy::too_many_arguments)]".to_string()),
                    Line(format!("pub fn set(&mut self{set_types}) {{")),
                    indent(Line(format!(" {set_inner}"))),
                    Line("}".to_string()),
                ])
            };
            if !is_params_struct {
                params_struct_string.push_str(rust_struct_inner);
                params_struct_impl_string.push_str(rust_struct_impl_inner);
                params_struct_impl_string.push_str("  \n}}");
                if !params_struct_string.is_empty() {
                    params_struct_string.push_str("  \n}");
                }
                output.push(Branch(vec![
                    Line(params_struct_string),
                    Line(params_struct_impl_string),
                    Line(params_enum_string),
                ]));
            }

            let builder_struct_size = Branch(vec![
                Line(fmt!(
                    ctx,
                    "impl <'a,{0}> {capnp}::traits::HasStructSize for Builder<'a,{0}> {1} {{",
                    params.params,
                    params.where_clause
                )),
                indent(Line(fmt!(
                    ctx,
                    "const STRUCT_SIZE: {capnp}::private::layout::StructSize = {capnp}::private::layout::StructSize {{ data: {}, pointers: {} }};",
                    data_size as usize,
                    pointer_size as usize
                ))),
                line("}"),
            ]);

            private_mod_interior.push(Line(format!(
                "pub const TYPE_ID: u64 = {};",
                format_u64(node_id)
            )));

            let from_pointer_builder_impl = Branch(vec![
                Line(fmt!(
                    ctx,
                    "impl <'a,{0}> {capnp}::traits::FromPointerBuilder<'a> for Builder<'a,{0}> {1} {{",
                    params.params,
                    params.where_clause
                )),
                indent(vec![
                    Line(fmt!(
                        ctx,
                        "fn init_pointer(builder: {capnp}::private::layout::PointerBuilder<'a>, _size: u32) -> Self {{"
                    )),
                    indent(Line(fmt!(
                        ctx,
                        "builder.init_struct(<Self as {capnp}::traits::HasStructSize>::STRUCT_SIZE).into()"
                    ))),
                    line("}"),
                    Line(fmt!(
                        ctx,
                        "fn get_from_pointer(builder: {capnp}::private::layout::PointerBuilder<'a>, default: ::core::option::Option<&'a [{capnp}::Word]>) -> {capnp}::Result<Self> {{"
                    )),
                    indent(Line(fmt!(
                        ctx,
                        "::core::result::Result::Ok(builder.get_struct(<Self as {capnp}::traits::HasStructSize>::STRUCT_SIZE, default)?.into())"
                    ))),
                    line("}"),
                ]),
                line("}"),
                BlankLine,
            ]);

            let accessors = vec![
                Branch(preamble),
                (if !is_generic {
                    Branch(vec![
                        Line("#[derive(Copy, Clone)]".into()),
                        line("pub struct Owned(());"),
                        Line(fmt!(ctx,"impl {capnp}::introspect::Introspect for Owned {{ fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Struct({capnp}::introspect::RawBrandedStructSchema {{ generic: &_private::RAW_SCHEMA, dynamic_schema: Option::None, field_types: _private::get_field_types, annotation_types: _private::get_annotation_types }}).into() }} }}")),
                        Line(fmt!(ctx, "impl {capnp}::traits::Owned for Owned {{ type Reader<'a> = Reader<'a>; type Builder<'a> = Builder<'a>; }}")),
                        Line(fmt!(ctx,"impl {capnp}::traits::OwnedStruct for Owned {{ type Reader<'a> = Reader<'a>; type Builder<'a> = Builder<'a>; }}")),
                        Line(fmt!(ctx,"impl {capnp}::traits::Pipelined for Owned {{ type Pipeline = Pipeline; }}"))
                    ])
                } else {
                    Branch(vec![
                        Line("#[derive(Copy, Clone)]".into()),
                        Line(format!("pub struct Owned<{}> {{", params.params)),
                            indent(Line(params.phantom_data_type.clone())),
                        line("}"),
                        Line(fmt!(ctx,"impl <{0}> {capnp}::introspect::Introspect for Owned <{0}> {1} {{ fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Struct({capnp}::introspect::RawBrandedStructSchema {{ generic: &_private::RAW_SCHEMA, dynamic_schema: Option::None, field_types: _private::get_field_types::<{0}>, annotation_types: _private::get_annotation_types::<{0}> }}).into() }} }}",
                            params.params, params.where_clause)),
                        Line(fmt!(ctx,"impl <{0}> {capnp}::traits::Owned for Owned <{0}> {1} {{ type Reader<'a> = Reader<'a, {0}>; type Builder<'a> = Builder<'a, {0}>; }}",
                            params.params, params.where_clause)),
                        Line(fmt!(ctx,"impl <{0}> {capnp}::traits::OwnedStruct for Owned <{0}> {1} {{ type Reader<'a> = Reader<'a, {0}>; type Builder<'a> = Builder<'a, {0}>; }}",
                            params.params, params.where_clause)),
                        Line(fmt!(ctx,"impl <{0}> {capnp}::traits::Pipelined for Owned<{0}> {1} {{ type Pipeline = Pipeline{2}; }}",
                            params.params, params.where_clause, bracketed_params)),
                    ])
                }),
                BlankLine,
                (if !is_generic {
                    Line(fmt!(ctx,"pub struct Reader<'a> {{ reader: {capnp}::private::layout::StructReader<'a> }}"))
                } else {
                    Branch(vec![
                        Line(format!("pub struct Reader<'a,{}> {} {{", params.params, params.where_clause)),
                        indent(vec![
                            Line(fmt!(ctx,"reader: {capnp}::private::layout::StructReader<'a>,")),
                            Line(params.phantom_data_type.clone()),
                        ]),
                        line("}")
                    ])
                }),
                // Manually implement Copy/Clone because `derive` only kicks in if all of
                // the parameters are known to implement Copy/Clone.
                Branch(vec![
                    Line(format!("impl <'a,{0}> ::core::marker::Copy for Reader<'a,{0}> {1} {{}}",
                                 params.params, params.where_clause)),
                    Line(format!("impl <'a,{0}> ::core::clone::Clone for Reader<'a,{0}> {1} {{",
                                 params.params, params.where_clause)),
                    indent(Line("fn clone(&self) -> Self { *self }".into())),
                    Line("}".into())]),
                BlankLine,
                Branch(vec![
                        Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::HasTypeId for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                        indent(vec![line("const TYPE_ID: u64 = _private::TYPE_ID;")]),
                    line("}")]),
                Line(fmt!(ctx,"impl <'a,{0}> ::core::convert::From<{capnp}::private::layout::StructReader<'a>> for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                indent(vec![
                    Line(fmt!(ctx,"fn from(reader: {capnp}::private::layout::StructReader<'a>) -> Self {{")),
                    indent(Line(format!("Self {{ reader, {} }}", params.phantom_data_value))),
                    line("}")
                ]),
                line("}"),
                BlankLine,
                Line(fmt!(ctx,"impl <'a,{0}> ::core::convert::From<Reader<'a,{0}>> for {capnp}::dynamic_value::Reader<'a> {1} {{",
                            params.params, params.where_clause)),
                indent(vec![
                    Line(format!("fn from(reader: Reader<'a,{0}>) -> Self {{", params.params)),
                    indent(Line(fmt!(ctx,"Self::Struct({capnp}::dynamic_struct::Reader::new(reader.reader, {capnp}::schema::StructSchema::new({capnp}::introspect::RawBrandedStructSchema {{ generic: &_private::RAW_SCHEMA, dynamic_schema: Option::None, field_types: _private::get_field_types::<{0}>, annotation_types: _private::get_annotation_types::<{0}>}})))", params.params))),
                    line("}")
                ]),
                line("}"),
                BlankLine,
                Line(format!("impl <'a,{0}> ::core::fmt::Debug for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                indent(vec![
                    Line("fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::result::Result<(), ::core::fmt::Error> {".into()),
                    indent(Line(fmt!(ctx,"core::fmt::Debug::fmt(&::core::convert::Into::<{capnp}::dynamic_value::Reader<'_>>::into(*self), f)"))),
                    line("}")
                ]),
                line("}"),

                BlankLine,

                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::FromPointerReader<'a> for Reader<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(vec![
                    Line(fmt!(ctx,"fn get_from_pointer(reader: &{capnp}::private::layout::PointerReader<'a>, default: ::core::option::Option<&'a [{capnp}::Word]>) -> {capnp}::Result<Self> {{")),
                    indent(line("::core::result::Result::Ok(reader.get_struct(default)?.into())")),
                    line("}")
                ]),
                line("}"),
                BlankLine,
                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::IntoInternalStructReader<'a> for Reader<'a,{0}> {1} {{",
                            params.params, params.where_clause)),
                indent(vec![
                    Line(fmt!(ctx,"fn into_internal_struct_reader(self) -> {capnp}::private::layout::StructReader<'a> {{")),
                    indent(line("self.reader")),
                    line("}")
                ]),
                line("}"),
                BlankLine,
                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::Imbue<'a> for Reader<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(vec![
                    Line(fmt!(ctx,"fn imbue(&mut self, cap_table: &'a {capnp}::private::layout::CapTable) {{")),
                    indent(Line(fmt!(ctx,"self.reader.imbue({capnp}::private::layout::CapTableReader::Plain(cap_table))"))),
                    line("}")
                ]),
                line("}"),
                BlankLine,
                Line(format!("impl <'a,{0}> Reader<'a,{0}> {1} {{", params.params, params.where_clause)),
                indent(vec![
                        Line(format!("pub fn reborrow(&self) -> Reader<'_,{}> {{",params.params)),
                        indent(line("Self { .. *self }")),
                        line("}"),
                        BlankLine,
                        Line(fmt!(ctx,"pub fn total_size(&self) -> {capnp}::Result<{capnp}::MessageSize> {{")),
                        indent(line("self.reader.total_size()")),
                        line("}")]),
                indent(reader_members),
                line("}"),
                BlankLine,
                (if !is_generic {
                    Line(fmt!(ctx,"pub struct Builder<'a> {{ builder: {capnp}::private::layout::StructBuilder<'a> }}"))
                } else {
                    Branch(vec![
                        Line(format!("pub struct Builder<'a,{}> {} {{",
                                     params.params, params.where_clause)),
                            indent(vec![
                            Line(fmt!(ctx, "builder: {capnp}::private::layout::StructBuilder<'a>,")),
                            Line(params.phantom_data_type.clone()),
                        ]),
                        line("}")
                    ])
                }),
                builder_struct_size,
                Branch(vec![
                    Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::HasTypeId for Builder<'a,{0}> {1} {{",
                                 params.params, params.where_clause)),
                    indent(vec![
                        line("const TYPE_ID: u64 = _private::TYPE_ID;")]),
                    line("}")
                ]),
                Line(fmt!(ctx,
                    "impl <'a,{0}> ::core::convert::From<{capnp}::private::layout::StructBuilder<'a>> for Builder<'a,{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(vec![
                        Line(fmt!(ctx,"fn from(builder: {capnp}::private::layout::StructBuilder<'a>) -> Self {{")),
                        indent(Line(format!("Self {{ builder, {} }}", params.phantom_data_value))),
                        line("}")
                ]),
                line("}"),
                BlankLine,
                Line(fmt!(ctx,"impl <'a,{0}> ::core::convert::From<Builder<'a,{0}>> for {capnp}::dynamic_value::Builder<'a> {1} {{",
                            params.params, params.where_clause)),
                indent(vec![
                        Line(format!("fn from(builder: Builder<'a,{0}>) -> Self {{", params.params)),
                        indent(Line(fmt!(ctx,"Self::Struct({capnp}::dynamic_struct::Builder::new(builder.builder, {capnp}::schema::StructSchema::new({capnp}::introspect::RawBrandedStructSchema {{ generic: &_private::RAW_SCHEMA, dynamic_schema: Option::None, field_types: _private::get_field_types::<{0}>, annotation_types: _private::get_annotation_types::<{0}>}})))", params.params))),
                        line("}")
                ]),
                line("}"),
                BlankLine,

                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::ImbueMut<'a> for Builder<'a,{0}> {1} {{",
                             params.params, params.where_clause)),
                indent(vec![
                        Line(fmt!(ctx,"fn imbue_mut(&mut self, cap_table: &'a mut {capnp}::private::layout::CapTable) {{")),
                        indent(Line(fmt!(ctx,"self.builder.imbue({capnp}::private::layout::CapTableBuilder::Plain(cap_table))"))),
                        line("}")]),
                line("}"),
                BlankLine,

                from_pointer_builder_impl,
                Line(fmt!(ctx,
                    "impl <{0}> {capnp}::traits::SetPointerBuilder for Reader<'_,{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(Line(fmt!(ctx,"fn set_pointer_builder(mut pointer: {capnp}::private::layout::PointerBuilder<'_>, value: Self, canonicalize: bool) -> {capnp}::Result<()> {{ pointer.set_struct(&value.reader, canonicalize) }}"))),
                line("}"),
                BlankLine,
                Line(format!("impl <'a,{0}> Builder<'a,{0}> {1} {{", params.params, params.where_clause)),
                indent(vec![
                        Line(format!("pub fn into_reader(self) -> Reader<'a,{}> {{", params.params)),
                        indent(line("self.builder.into_reader().into()")),
                        line("}"),
                        Line(format!("pub fn reborrow(&mut self) -> Builder<'_,{}> {{", params.params)),
                        (if !is_generic {
                            indent(line("Builder { builder: self.builder.reborrow() }"))
                        } else {
                            indent(line("Builder { builder: self.builder.reborrow(), ..*self }"))
                        }),
                        line("}"),
                        Line(format!("pub fn reborrow_as_reader(&self) -> Reader<'_,{}> {{", params.params)),
                        indent(line("self.builder.as_reader().into()")),
                        line("}"),
                        BlankLine,
                        Line(fmt!(ctx,"pub fn total_size(&self) -> {capnp}::Result<{capnp}::MessageSize> {{")),
                        indent(line("self.builder.as_reader().total_size()")),
                        line("}")
                        ]),
                indent(set),
                indent(builder_members),
                line("}"),
                BlankLine,
                (if is_generic {
                    Branch(vec![
                        Line(format!("pub struct Pipeline{bracketed_params} {{")),
                        indent(vec![
                            Line(fmt!(ctx,"_typeless: {capnp}::any_pointer::Pipeline,")),
                            Line(params.phantom_data_type),
                        ]),
                        line("}")
                    ])
                } else {
                    Line(fmt!(ctx,"pub struct Pipeline {{ _typeless: {capnp}::any_pointer::Pipeline }}"))
                }),
                Line(fmt!(ctx,"impl{bracketed_params} {capnp}::capability::FromTypelessPipeline for Pipeline{bracketed_params} {{")),
                indent(vec![
                        Line(fmt!(ctx,"fn new(typeless: {capnp}::any_pointer::Pipeline) -> Self {{")),
                        indent(Line(format!("Self {{ _typeless: typeless, {} }}", params.phantom_data_value))),
                        line("}")]),
                line("}"),
                Line(format!("impl{0} Pipeline{0} {1} {{", bracketed_params,
                            params.pipeline_where_clause)),
                indent(pipeline_impl_interior),
                line("}"),
                line("mod _private {"),
                indent(private_mod_interior),
                line("}"),
            ];

            output.push(indent(vec![
                Branch(accessors),
                Branch(which_enums),
                Branch(nested_output),
            ]));
            output.push(line("}"));
        }

        node::Enum(enum_reader) => {
            let last_name = ctx.get_last_name(node_id)?;
            let name_as_mod = module_name(last_name);
            output.push(BlankLine);

            let mut members = Vec::new();
            let mut match_branches = Vec::new();
            let enumerants = enum_reader.get_enumerants()?;
            for (ii, enumerant) in enumerants.into_iter().enumerate() {
                let enumerant = capitalize_first_letter(get_enumerant_name(enumerant)?);
                members.push(Line(format!("{enumerant} = {ii},")));
                match_branches.push(Line(format!(
                    "{ii} => ::core::result::Result::Ok(Self::{enumerant}),"
                )));
            }
            match_branches.push(Line(fmt!(
                ctx,
                "n => ::core::result::Result::Err({capnp}::NotInSchema(n)),"
            )));

            output.push(Branch(vec![
                line("#[repr(u16)]"),
                line("#[derive(Clone, Copy, Debug, PartialEq, Eq)]"),
                Line(format!("pub enum {last_name} {{")),
                indent(members),
                line("}"),
            ]));

            output.push(BlankLine);
            output.push(Branch(vec![
                Line(fmt!(ctx,
                    "impl {capnp}::introspect::Introspect for {last_name} {{"
                )),
                indent(Line(fmt!(ctx,
                    "fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Enum({capnp}::introspect::RawEnumSchema {{ encoded_node: &{0}::ENCODED_NODE, annotation_types: {0}::get_annotation_types }}).into() }}", name_as_mod))),
                Line("}".into()),
            ]));

            output.push(Branch(vec![
                Line(fmt!(ctx,"impl ::core::convert::From<{last_name}> for {capnp}::dynamic_value::Reader<'_> {{")),
                indent(Line(fmt!(ctx,
                    "fn from(e: {last_name}) -> Self {{ {capnp}::dynamic_value::Enum::new(e.into(), {capnp}::introspect::RawEnumSchema {{ encoded_node: &{0}::ENCODED_NODE, annotation_types: {0}::get_annotation_types }}.into()).into() }}", name_as_mod ))),
                Line("}".into())
            ]));

            output.push(Branch(vec![
                Line(format!(
                    "impl ::core::convert::TryFrom<u16> for {last_name} {{"
                )),
                indent(Line(
                    fmt!(ctx,"type Error = {capnp}::NotInSchema;"),
                )),
                indent(vec![
                    Line(
                        format!("fn try_from(value: u16) -> ::core::result::Result<Self, <{last_name} as ::core::convert::TryFrom<u16>>::Error> {{")
                    ),
                    indent(vec![
                        line("match value {"),
                        indent(match_branches),
                        line("}"),
                    ]),
                    line("}"),
                ]),
                line("}"),
                Line(format!("impl From<{last_name}> for u16 {{")),
                indent(line("#[inline]")),
                indent(Line(format!(
                    "fn from(x: {last_name}) -> u16 {{ x as u16 }}"
                ))),
                line("}"),
            ]));

            output.push(Branch(vec![
                Line(fmt!(
                    ctx,
                    "impl {capnp}::traits::HasTypeId for {last_name} {{"
                )),
                indent(Line(format!(
                    "const TYPE_ID: u64 = {}u64;",
                    format_u64(node_id)
                ))),
                line("}"),
            ]));

            output.push(Branch(vec![
                Line(format!("mod {name_as_mod} {{",)),
                Branch(vec![
                    crate::pointer_constants::node_word_array_declaration(
                        ctx,
                        "ENCODED_NODE",
                        *node_reader,
                        crate::pointer_constants::WordArrayDeclarationOptions { public: true },
                    )?,
                    generate_get_annotation_types(ctx, *node_reader)?,
                ]),
                Line("}".into()),
            ]));
        }

        node::Interface(interface) => {
            let params = node_reader.parameters_texts(ctx);
            output.push(BlankLine);

            let is_generic = node_reader.get_is_generic();

            let names = &ctx.scope_map[&node_id];
            let mut client_impl_interior = Vec::new();
            let mut server_interior = Vec::new();
            let mut mod_interior = Vec::new();
            let mut dispatch_arms = Vec::new();
            let mut private_mod_interior = Vec::new();
            let client_implicit = get_params(ctx, node_id)?;

            let bracketed_params = if params.params.is_empty() {
                "".to_string()
            } else {
                format!("<{}>", params.params)
            };

            private_mod_interior.push(Line(format!(
                "pub const TYPE_ID: u64 = {};",
                format_u64(node_id)
            )));

            private_mod_interior.push(crate::pointer_constants::node_word_array_declaration(
                ctx,
                "ENCODED_NODE",
                *node_reader,
                crate::pointer_constants::WordArrayDeclarationOptions { public: true },
            )?);

            mod_interior.push(line("#![allow(unused_variables)]"));
            mod_interior.push(line("#![allow(clippy::extra_unused_type_parameters)]"));
            mod_interior.push(line("#![allow(clippy::wrong_self_convention)]"));
            mod_interior.push(line("#![allow(clippy::needless_lifetimes)]"));
            mod_interior.push(BlankLine);
            let methods = interface.get_methods()?;
            let mut method_count = 0;
            for (ordinal, method) in methods.into_iter().enumerate() {
                let name = method.get_name()?.to_str()?;
                method_count += 1;

                let param_id = method.get_param_struct_type();
                let param_node = &ctx.node_map[&param_id];
                let mut builder_params_string = String::new();
                let mut builder_params_inner_string = String::new();
                let mut params_generics = HashSet::new();
                let (param_scopes, params_ty_params) = if param_node.get_scope_id() == 0 {
                    let mut names = names.clone();
                    let local_name = module_name(&format!("{name}Params"));
                    nested_output.push(generate_node(
                        ctx,
                        param_id,
                        &local_name,
                        &mut builder_params_string,
                        &mut builder_params_inner_string,
                        &mut params_generics,
                        &client_implicit,
                        true,
                    )?);
                    names.push(local_name);
                    (names, params.params.clone())
                } else {
                    (
                        ctx.scope_map[&param_node.get_id()].clone(),
                        get_ty_params_of_brand(ctx, method.get_param_brand()?)?,
                    )
                };
                let param_type = do_branding(
                    ctx,
                    param_id,
                    method.get_param_brand()?,
                    Leaf::Owned,
                    &param_scopes.join("::"),
                )?;

                let result_id = method.get_result_struct_type();
                let result_node = &ctx.node_map[&result_id];
                let (result_scopes, results_ty_params) = if result_node.get_scope_id() == 0 {
                    let mut names = names.clone();
                    let local_name = module_name(&format!("{name}Results"));
                    nested_output.push(generate_node(
                        ctx,
                        result_id,
                        &local_name,
                        &mut String::new(),
                        &mut String::new(),
                        &mut HashSet::new(),
                        &Vec::new(),
                        true,
                    )?);
                    names.push(local_name);
                    (names, params.params.clone())
                } else {
                    (
                        ctx.scope_map[&result_node.get_id()].clone(),
                        get_ty_params_of_brand(ctx, method.get_result_brand()?)?,
                    )
                };
                let result_type = do_branding(
                    ctx,
                    result_id,
                    method.get_result_brand()?,
                    Leaf::Owned,
                    &result_scopes.join("::"),
                )?;

                dispatch_arms.push(
                    Line(fmt!(ctx,
                        "{ordinal} => self.server.{}({capnp}::private::capability::internal_get_typed_params(params), {capnp}::private::capability::internal_get_typed_results(results)).await,",
                        module_name(name))));
                mod_interior.push(Line(fmt!(
                    ctx,
                    "pub type {}Params<{}> = {capnp}::capability::Params<{}>;",
                    capitalize_first_letter(name),
                    params_ty_params,
                    param_type
                )));
                mod_interior.push(Line(fmt!(
                    ctx,
                    "pub type {}Results<{}> = {capnp}::capability::Results<{}>;",
                    capitalize_first_letter(name),
                    results_ty_params,
                    result_type
                )));
                server_interior.push(
                    Line(fmt!(ctx,
                        "async fn {}(self: std::rc::Rc<Self>, _: {}Params<{}>, _: {}Results<{}>) -> Result<(), {capnp}::Error> {{ Result::<(), {capnp}::Error>::Err({capnp}::Error::unimplemented(\"method {}::Server::{} not implemented\".to_string())) }}",
                        module_name(name),
                        capitalize_first_letter(name), params_ty_params,
                        capitalize_first_letter(name), results_ty_params,
                        node_name, module_name(name)
                    )));

                client_impl_interior.push(Line(fmt!(
                    ctx,
                    "pub fn {}_request(&self) -> {capnp}::capability::Request<{},{}> {{",
                    camel_to_snake_case(name),
                    param_type,
                    result_type
                )));

                client_impl_interior.push(indent(Line(format!(
                    "self.client.new_call(_private::TYPE_ID, {ordinal}, ::core::option::Option::None)"
                ))));
                client_impl_interior.push(line("}"));

                let params_type_string = format!(", {builder_params_string}");
                for implicit in &client_implicit {
                    params_generics.remove(implicit);
                }
                let mut builder_params = String::new();
                for generic in params_generics {
                    builder_params.push(',');
                    builder_params.push_str(generic.as_str());
                    builder_params.push_str(fmt!(ctx, ": {capnp}::traits::Owned").as_str());
                }
                client_impl_interior.push(Line(fmt!(
                    ctx,
                    "pub fn build_{}_request<'a{builder_params}>(&'a self{}) -> {capnp}::capability::Request<{},{}> {} {{",
                    camel_to_snake_case(name),
                    params_type_string,
                    param_type,
                    result_type,
                    params.where_clause
                )));

                client_impl_interior.push(indent(Line(fmt!(
                    ctx,
                    "let mut req: {capnp}::capability::Request<{},{}> = self.client.new_call(_private::TYPE_ID, {ordinal}, ::core::option::Option::None);\n      let mut _builder = req.get();{builder_params_inner_string}\n      req", 
                    param_type,
                    result_type
                ))));
                client_impl_interior.push(line("}"));

                method.get_annotations()?;
            }

            let mut base_dispatch_arms = Vec::new();
            let server_base = {
                let mut base_traits = Vec::new();

                fn find_super_interfaces<'a>(
                    interface: schema_capnp::node::interface::Reader<'a>,
                    all_extends: &mut Vec<
                        <schema_capnp::superclass::Owned as capnp::traits::OwnedStruct>::Reader<'a>,
                    >,
                    ctx: &GeneratorContext<'a>,
                ) -> ::capnp::Result<()> {
                    let extends = interface.get_superclasses()?;
                    for superclass in extends {
                        if let node::Interface(interface) =
                            ctx.node_map[&superclass.get_id()].which()?
                        {
                            find_super_interfaces(interface, all_extends, ctx)?;
                        }
                        all_extends.push(superclass);
                    }
                    Ok(())
                }

                let mut extends = Vec::new();
                find_super_interfaces(interface, &mut extends, ctx)?;
                for ext in &extends {
                    let type_id = ext.get_id();
                    let brand = ext.get_brand()?;
                    let the_mod = ctx.get_qualified_module(type_id);
                    base_dispatch_arms.push(Line(format!(
                        "0x{type_id:x} => Self::dispatch_call_internal(self, method_id + {method_count}, params, results).await,")));
                    base_traits.push(do_branding(ctx, type_id, brand, Leaf::Server, &the_mod)?);

                    let node_reader = &ctx.node_map[&ext.get_id()];
                    let node::Which::Interface(ext) = node_reader.which()? else {
                        return Err(capnp::Error::from_kind(capnp::ErrorKind::TypeMismatch));
                    };
                    let names = &ctx.scope_map[&node_reader.get_id()];
                    let methods = ext.get_methods()?;
                    for method in methods.into_iter() {
                        let name = method.get_name()?.to_str()?;
                        let mut builder_params_string = String::new();
                        let mut builder_params_impl_string = String::new();
                        let param_id = method.get_param_struct_type();
                        let mut used_params_in_method = HashSet::new();
                        let param_node = &ctx.node_map[&param_id];
                        let schema_capnp::node::Struct(struct_r) = param_node.which()? else {
                            return Err(capnp::Error::from_kind(capnp::ErrorKind::TypeMismatch));
                        };
                        let fields = struct_r.get_fields()?;
                        for field in fields {
                            let name = get_field_name(field)?;
                            let styled_name = camel_to_snake_case(name);
                            let no_discriminant =
                                field.get_discriminant_value() == field::NO_DISCRIMINANT;
                            match field.which()? {
                                field::Group(group) => {
                                    let the_mod = ctx.get_qualified_module(group.get_type_id());
                                    used_params_of_group(
                                        ctx,
                                        group.get_type_id(),
                                        &mut used_params_in_method,
                                    )?;
                                    if no_discriminant {
                                        builder_params_string.push_str(
                                            format!(
                                                "_{styled_name}: {}::{},",
                                                the_mod,
                                                snake_to_camel_case(
                                                    ctx.get_last_name(group.get_type_id())?
                                                )
                                            )
                                            .as_str(),
                                        );
                                        builder_params_impl_string.push_str(format!("\n  _{styled_name}.build_capnp_struct(_builder.reborrow().init_{styled_name}());").as_str());
                                    }
                                }
                                field::Slot(reg_field) => {
                                    let typ = reg_field.get_type()?;
                                    used_params_of_type(ctx, typ, &mut used_params_in_method)?;
                                    match typ.which()? {
                                        type_::Void(()) => {
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: (),").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        type_::Bool(()) => {
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: bool,").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        _ if typ.is_prim()? => {
                                            let tstr = typ.type_string(ctx, Leaf::Reader("'a"))?;
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: {tstr},").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        type_::Text(()) => {
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: &'a str,").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name}.into());").as_str());
                                            }
                                        }
                                        type_::Data(()) => {
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: &'a [u8],").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        type_::List(ot1) => {
                                            if no_discriminant {
                                                if let Ok(vec_of_list_element_types) =
                                                    vec_of_list_element_types(
                                                        ctx,
                                                        ot1.reborrow(),
                                                        &mut HashSet::new(),
                                                    )
                                                {
                                                    builder_params_string.push_str(
                                                        format!(
                                                            "_{styled_name}: {vec_of_list_element_types},",
                                                        )
                                                        .as_str(),
                                                    );
                                                    builder_params_impl_string.push_str(
                                                        build_impl_for_list_type(
                                                            styled_name.as_str(),
                                                            "_builder",
                                                            ot1.reborrow(),
                                                            false,
                                                            true,
                                                        )?
                                                        .as_str(),
                                                    );
                                                }
                                            }
                                        }
                                        type_::Enum(e) => {
                                            let id = e.get_type_id();
                                            let the_mod = ctx.get_qualified_module(id);
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!("_{styled_name}: {the_mod},").as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        type_::Struct(st) => {
                                            let type_string =
                                                get_params_struct_path_string(ctx, st)?;

                                            let mut lifetime = "";
                                            let node::Struct(struct_node) =
                                                ctx.node_map[&st.get_type_id()].which()?
                                            else {
                                                return Err(capnp::Error::failed(
                                                    "Type mismatch".to_string(),
                                                ));
                                            };
                                            check_fields_of_struct_for_lifetimes(
                                                ctx,
                                                struct_node.get_fields()?,
                                                &mut lifetime,
                                                0,
                                            )?;
                                            let the_mod =
                                                &ctx.get_qualified_module(st.get_type_id());
                                            let maybe_generics = do_branding(
                                                ctx,
                                                st.get_type_id(),
                                                st.get_brand()?,
                                                Leaf::Reader(""),
                                                the_mod,
                                            )?;
                                            let maybe_generics = &maybe_generics
                                                [the_mod.len() + 9..maybe_generics.len() - 1];
                                            if !lifetime.is_empty() {
                                                params_struct_generics.insert("'a".to_string());
                                                lifetime = "'a";
                                            }
                                            let bracketed_params = if !lifetime.is_empty()
                                                || maybe_generics.len() > 1
                                            {
                                                format! {"<{lifetime}{maybe_generics}>"}
                                            } else {
                                                "".to_string()
                                            };
                                            if no_discriminant {
                                                if type_string
                                                    .rfind(snake_to_camel_case(node_name).as_str())
                                                    .is_some()
                                                {
                                                    builder_params_string.push_str(
                                                        format!(
                                                            "_{styled_name}: Option<Box<{type_string}{bracketed_params}>>,",
                                                        )
                                                        .as_str(),
                                                    );
                                                    builder_params_impl_string.push_str(format!("\n  if let Some(st) = _{styled_name} {{st.build_capnp_struct(_builder.reborrow().init_{styled_name}());}}").as_str());
                                                } else {
                                                    builder_params_string.push_str(
                                                        format!(
                                                            "_{styled_name}: Option<{type_string}{bracketed_params}>,",
                                                        )
                                                        .as_str(),
                                                    );
                                                    builder_params_impl_string.push_str(format!("\n  if let Some(st) = _{styled_name} {{st.build_capnp_struct(_builder.reborrow().init_{styled_name}());}}").as_str());
                                                }
                                            }
                                        }
                                        type_::Interface(_) => {
                                            if no_discriminant {
                                                builder_params_string.push_str(
                                                    format!(
                                                        "_{styled_name}: {},",
                                                        typ.type_string(ctx, Leaf::Client)?
                                                    )
                                                    .as_str(),
                                                );
                                                builder_params_impl_string.push_str(format!("\n  _builder.set_{styled_name}(_{styled_name});").as_str());
                                            }
                                        }
                                        type_::AnyPointer(an) => {
                                            match an.which()? {
                                                type_::any_pointer::Which::Unconstrained(_) => {
                                                    //TODO implement for anypointers besides caps
                                                    builder_params_string.push_str(fmt!(ctx, "_{styled_name}: Box<dyn {capnp}::private::capability::ClientHook>,").as_str());
                                                    builder_params_impl_string.push_str(format!("\n  _builder.reborrow().init_{styled_name}().set_as_capability(_{styled_name});").as_str());
                                                },
                                                type_::any_pointer::Which::Parameter(p) => {
                                                    let reader_type =
                                                    typ.type_string(ctx, Leaf::Reader("'a"))?;
                                                    builder_params_string.push_str(
                                                        format!("_{styled_name}: {reader_type},")
                                                            .as_str(),
                                                    );
                                                    let mut implicit = false;
                                                    let the_struct = &ctx.node_map[&p.get_scope_id()];
                                                    let parameters = the_struct.get_parameters()?;
                                                    let parameter = parameters.get(u32::from(p.get_parameter_index()));
                                                    let parameter_name = parameter.get_name()?.to_str()?;
                                                    for par in method.get_implicit_parameters()?.iter() {
                                                        if par.get_name()?.to_str()? == parameter_name {
                                                            implicit = true;
                                                        }
                                                    }
                                                    if !implicit {
                                                        builder_params_impl_string.push_str(format!("\n      _builder.reborrow().set_{styled_name}(_{styled_name}).unwrap();").as_str());
                                                    } else {
                                                        builder_params_impl_string.push_str(format!("\n      _builder.reborrow().init_{styled_name}().set_as(_{styled_name}).unwrap();").as_str());
                                                    }
                                                },
                                                type_::any_pointer::Which::ImplicitMethodParameter(_) => (),
                                            }
                                        }
                                        _ => {
                                            return Err(Error::failed(
                                                "unrecognized type".to_string(),
                                            ));
                                        }
                                    }
                                }
                            }
                        }

                        let param_scopes = if param_node.get_scope_id() == 0 {
                            let mut names = names.clone();
                            let local_name = module_name(&format!("{name}Params"));
                            names.push(local_name);
                            names
                        } else {
                            ctx.scope_map[&param_node.get_id()].clone()
                        };
                        let param_type = do_branding(
                            ctx,
                            param_id,
                            method.get_param_brand()?,
                            Leaf::Owned,
                            &param_scopes.join("::"),
                        )?;

                        let result_id = method.get_result_struct_type();
                        let result_node = &ctx.node_map[&result_id];
                        let result_scopes = if result_node.get_scope_id() == 0 {
                            let mut names = names.clone();
                            let local_name = module_name(&format!("{name}Results"));
                            names.push(local_name);
                            names
                        } else {
                            ctx.scope_map[&result_node.get_id()].clone()
                        };
                        let result_type = do_branding(
                            ctx,
                            result_id,
                            method.get_result_brand()?,
                            Leaf::Owned,
                            &result_scopes.join("::"),
                        )?;

                        used_params_of_brand(
                            ctx,
                            type_id,
                            method.get_param_brand()?,
                            &mut used_params_in_method,
                        )?;
                        used_params_of_brand(
                            ctx,
                            type_id,
                            method.get_result_brand()?,
                            &mut used_params_in_method,
                        )?;
                        for par in &params.expanded_list {
                            used_params_in_method.remove(par);
                        }
                        let mut extra_params = Vec::new();
                        for par in used_params_in_method {
                            extra_params.push(fmt!(ctx, "{par}: {capnp}::traits::Owned"));
                        }

                        client_impl_interior.push(Line(fmt!(
                            ctx,
                            "pub fn {}_request<'a,{}>(&'a self) -> {capnp}::capability::Request<{},{}> {{",
                            camel_to_snake_case(name),
                            extra_params.join(","),
                            param_type,
                            result_type
                        )));

                        client_impl_interior.push(indent(Line(format!(
                            "self.client.new_call(_private::TYPE_ID, {method_count}, ::core::option::Option::None)"
                        ))));
                        client_impl_interior.push(line("}"));

                        client_impl_interior.push(Line(fmt!(ctx,
                            "pub fn build_{}_request<'a,{}>(&'a self, {}) -> {capnp}::capability::Request<{},{}> {} {{",
                            camel_to_snake_case(name),
                            extra_params.join(","),
                            builder_params_string,
                            param_type,
                            result_type,
                            params.where_clause
                        )));

                        client_impl_interior.push(indent(Line(fmt!(ctx,
                            "let mut req: {capnp}::capability::Request<{},{}> = self.client.new_call(_private::TYPE_ID, {method_count}, ::core::option::Option::None);\n      let mut _builder = req.get();\n      {}\n      req",
                            param_type,
                            result_type,
                            builder_params_impl_string
                        ))));
                        client_impl_interior.push(line("}"));

                        dispatch_arms.push(
                            Line(fmt!(ctx,
                                "{method_count} => self.server.{}({capnp}::private::capability::internal_get_typed_params(params), {capnp}::private::capability::internal_get_typed_results(results)).await,",
                                module_name(name))));
                        method_count += 1;
                    }
                }
                if !extends.is_empty() {
                    format!(": {}", base_traits.join(" + "))
                } else {
                    "".to_string()
                }
            };

            mod_interior.push(BlankLine);
            mod_interior.push(Line(format!("pub struct Client{bracketed_params} {{")));
            mod_interior.push(indent(Line(fmt!(
                ctx,
                "pub client: {capnp}::capability::Client,"
            ))));
            if is_generic {
                mod_interior.push(indent(Line(params.phantom_data_type.clone())));
            }
            mod_interior.push(line("}"));
            mod_interior.push(
                Branch(vec![
                    Line(fmt!(ctx,"impl {bracketed_params} {capnp}::capability::FromClientHook for Client{bracketed_params} {} {{", params.where_clause)),
                    indent(Line(fmt!(ctx,"fn new(hook: Box<dyn ({capnp}::private::capability::ClientHook)>) -> Self {{"))),
                    indent(indent(Line(fmt!(ctx,"Self {{ client: {capnp}::capability::Client::new(hook), {} }}", params.phantom_data_value)))),
                    indent(line("}")),
                    indent(Line(fmt!(ctx,"fn into_client_hook(self) -> Box<dyn ({capnp}::private::capability::ClientHook)> {{"))),
                    indent(indent(line("self.client.hook"))),
                    indent(line("}")),
                    indent(Line(fmt!(ctx,"fn as_client_hook(&self) -> &dyn ({capnp}::private::capability::ClientHook) {{"))),
                    indent(indent(line("&*self.client.hook"))),
                    indent(line("}")),
                    line("}"),
                    Line(fmt!(ctx,"impl {bracketed_params} {capnp}::introspect::Introspect for Client{bracketed_params} {} {{ fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Capability({capnp}::introspect::RawCapabilitySchema {{ ", params.where_clause)),
                    indent(Line("encoded_node: &_private::ENCODED_NODE,".to_string())),
                    indent(Line(format!("params_types: _private::get_param_type::<{}>,", params.params))),
                    indent(Line(format!("result_types: _private::get_result_type::<{}> }}).into() }}  }}", params.params))),
                    ]));

            mod_interior.push(if !is_generic {
                Branch(vec![
                    Line("#[derive(Copy, Clone)]".into()),
                    line("pub struct Owned(());"),
                    Line(fmt!(ctx,"impl {capnp}::introspect::Introspect for Owned {{ fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Capability({capnp}::introspect::RawCapabilitySchema {{ ")),
                    indent(Line("encoded_node: &_private::ENCODED_NODE,".to_string())),
                    indent(Line("params_types: _private::get_param_type,".to_string())),
                    indent(Line("result_types: _private::get_result_type }).into()}}".to_string())),
                    line(fmt!(ctx,"impl {capnp}::traits::Owned for Owned {{ type Reader<'a> = Client; type Builder<'a> = Client; }}")),
                    Line(fmt!(ctx,"impl {capnp}::traits::Pipelined for Owned {{ type Pipeline = Client; }}"))])
            } else {
                Branch(vec![
                    Line("#[derive(Copy, Clone)]".into()),
                    Line(format!("pub struct Owned<{}> {} {{", params.params, params.where_clause)),
                    indent(Line(params.phantom_data_type.clone())),
                    line("}"),
                    Line(fmt!(ctx,
                              "impl <{0}> {capnp}::introspect::Introspect for Owned <{0}> {1} {{ fn introspect() -> {capnp}::introspect::Type {{ {capnp}::introspect::TypeVariant::Capability({capnp}::introspect::RawCapabilitySchema {{ ",
                              params.params, params.where_clause)),
                    indent(Line("encoded_node: &_private::ENCODED_NODE,".to_string())),
                    indent(Line(format!("params_types: _private::get_param_type::<{}>,", params.params))),
                    indent(Line(format!("result_types: _private::get_result_type::<{}> }}).into() }} }}", params.params))),
                    Line(fmt!(ctx,
                        "impl <{0}> {capnp}::traits::Owned for Owned <{0}> {1} {{ type Reader<'a> = Client<{0}>; type Builder<'a> = Client<{0}>; }}",
                        params.params, params.where_clause)),
                    Line(fmt!(ctx,
                        "impl <{0}> {capnp}::traits::Pipelined for Owned <{0}> {1} {{ type Pipeline = Client{2}; }}",
                        params.params, params.where_clause, bracketed_params))])
            });

            mod_interior.push(Branch(vec![
                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::FromPointerReader<'a> for Client<{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(vec![
                        Line(fmt!(ctx,"fn get_from_pointer(reader: &{capnp}::private::layout::PointerReader<'a>, _default: ::core::option::Option<&'a [{capnp}::Word]>) -> {capnp}::Result<Self> {{")),
                        indent(Line(fmt!(ctx,"::core::result::Result::Ok({capnp}::capability::FromClientHook::new(reader.get_capability()?))"))),
                        line("}")]),
                line("}")]));

            mod_interior.push(Branch(vec![
                Line(fmt!(ctx,"impl <'a,{0}> {capnp}::traits::FromPointerBuilder<'a> for Client<{0}> {1} {{",
                             params.params, params.where_clause)),
                indent(vec![
                            Line(fmt!(ctx,"fn init_pointer(_builder: {capnp}::private::layout::PointerBuilder<'a>, _size: u32) -> Self {{")),
                            indent(line("unimplemented!()")),
                            line("}"),
                            Line(fmt!(ctx,"fn get_from_pointer(builder: {capnp}::private::layout::PointerBuilder<'a>, _default: ::core::option::Option<&'a [{capnp}::Word]>) -> {capnp}::Result<Self> {{")),
                            indent(Line(fmt!(ctx,"::core::result::Result::Ok({capnp}::capability::FromClientHook::new(builder.get_capability()?))"))),
                            line("}")]),
                line("}"),
                BlankLine]));

            mod_interior.push(Branch(vec![
                Line(fmt!(ctx,
                    "impl <{0}> {capnp}::traits::SetPointerBuilder for Client<{0}> {1} {{",
                    params.params, params.where_clause)),
                indent(vec![
                            Line(fmt!(ctx,"fn set_pointer_builder(mut pointer: {capnp}::private::layout::PointerBuilder<'_>, from: Self, _canonicalize: bool) -> {capnp}::Result<()> {{")),
                            indent(Line("pointer.set_capability(from.client.hook);".to_string())),
                            indent(Line("::core::result::Result::Ok(())".to_string())),
                            line("}")
                        ]
                ),
                line("}")]));

            mod_interior.push(Branch(vec![
                Line(fmt!(ctx,
                    "impl {bracketed_params} {capnp}::traits::HasTypeId for Client{bracketed_params} {{"
                )),
                indent(Line(
                    "const TYPE_ID: u64 = _private::TYPE_ID;".to_string(),
                )),
                line("}"),
            ]));

            mod_interior.push(
                Branch(vec![
                    Line(format!("impl {bracketed_params} Clone for Client{bracketed_params} {{")),
                    indent(line("fn clone(&self) -> Self {")),
                    indent(indent(Line(fmt!(ctx,"Self {{ client: {capnp}::capability::Client::new(self.client.hook.add_ref()), {} }}", params.phantom_data_value)))),
                    indent(line("}")),
                    line("}")]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "impl {bracketed_params} Client{bracketed_params} {{"
                )),
                indent(client_impl_interior),
                line("}"),
            ]));

            mod_interior.push(Branch(vec![
                line("#[allow(async_fn_in_trait)]"),
                Line(format!(
                    "pub trait Server<{}> {} {} {{",
                    params.params, server_base, params.where_clause
                )),
                indent(server_interior),
                line("}"),
            ]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "pub struct ServerDispatch<_T,{}> {{",
                    params.params
                )),
                indent(line("pub server: std::rc::Rc<_T>,")),
                indent(if is_generic {
                    vec![Line(params.phantom_data_type.clone())]
                } else {
                    vec![]
                }),
                line("}"),
            ]));

            mod_interior.push(Branch(vec![
                Line(format!(
                    "impl <_S: Server{1} + 'static, {0}> Clone for ServerDispatch<_S, {0}> {2} {{",
                    params.params, bracketed_params, params.where_clause
                )),
                indent(vec![
                    Line("fn clone(&self) -> Self {".to_string()),
                    indent(Line(format!(
                        "ServerDispatch {{ server: self.server.clone(), {} }}",
                        params.phantom_data_value
                    ))),
                    line("}"),
                ]),
                line("}"),
            ]));

            mod_interior.push(Branch(vec![
                Line(
                    fmt!(ctx,"impl <_S: Server{1} + 'static, {0}> {capnp}::capability::FromServer<_S> for Client{1} {2}  {{",
                            params.params, bracketed_params, params.where_clause_with_static)),
                indent(vec![
                    Line(format!("type Dispatch = ServerDispatch<_S, {}>;", params.params)),
                    Line(format!("fn from_server(s: _S) -> ServerDispatch<_S, {}> {{", params.params)),
                    indent(Line(format!("ServerDispatch {{ server: std::rc::Rc::new(s), {} }}", params.phantom_data_value))),
                    line("}"),
                    Line(format!("fn from_rc(s: std::rc::Rc<_S>) -> ServerDispatch<_S, {}> {{", params.params)),
                    indent(Line(format!("ServerDispatch {{ server: s, {} }}", params.phantom_data_value))),
                    line("}"),
                ]),
                line("}"),
            ]));

            mod_interior.push(
                Branch(vec![
                    (if is_generic {
                        Line(format!("impl <{}, _T: Server{}> ::core::ops::Deref for ServerDispatch<_T,{}> {} {{", params.params, bracketed_params, params.params, params.where_clause))
                    } else {
                        line("impl <_T: Server> ::core::ops::Deref for ServerDispatch<_T> {")
                    }),
                    indent(line("type Target = _T;")),
                    indent(line("fn deref(&self) -> &_T { self.server.as_ref() }")),
                    line("}"),
                    ]));

            mod_interior.push(
                Branch(vec![
                    (if is_generic {
                        Line(fmt!(ctx,"impl <{}, _T: Server{}> {capnp}::capability::Server for ServerDispatch<_T,{}> {} {{", params.params, bracketed_params, params.params, params.where_clause))
                    } else {
                        Line(fmt!(ctx,"impl <_T: Server> {capnp}::capability::Server for ServerDispatch<_T> {{"))
                    }),
                    indent(Line(fmt!(ctx,"async fn dispatch_call(self, interface_id: u64, method_id: u16, params: {capnp}::capability::Params<{capnp}::any_pointer::Owned>, results: {capnp}::capability::Results<{capnp}::any_pointer::Owned>) -> Result<(), {capnp}::Error> {{"))),
                    indent(indent(line("match interface_id {"))),
                    indent(indent(indent(line("_private::TYPE_ID => Self::dispatch_call_internal(self, method_id, params, results).await,")))),
                    indent(indent(indent(base_dispatch_arms.clone()))),
                    indent(indent(indent(Line(fmt!(ctx,"_ =>  Err({capnp}::Error::unimplemented(\"Method not implemented.\".to_string())) "))))),
                    indent(indent(line("}"))),
                    indent(line("}")),
                    indent(line("fn get_ptr(&self) -> usize {")),
                    indent(indent(line("std::rc::Rc::<_T>::as_ptr(&self.server) as usize"))),
                    indent(line("}")),
                    line("}")]));

            mod_interior.push(
                Branch(vec![
                    (if is_generic {
                        Line(format!("impl <{}, _T: Server{}> ServerDispatch<_T,{}> {} {{", params.params, bracketed_params, params.params, params.where_clause))
                    } else {
                        line("impl <_T :Server> ServerDispatch<_T> {")
                    }),
                    indent(Line(fmt!(ctx,"pub async fn dispatch_call_internal(self, method_id: u16, params: {capnp}::capability::Params<{capnp}::any_pointer::Owned>, results: {capnp}::capability::Results<{capnp}::any_pointer::Owned>) -> Result<(), {capnp}::Error> {{"))),
                    (if !dispatch_arms.is_empty() {
                        indent(vec![
                            (indent(indent(line("match method_id {")))),
                            (indent(indent(indent(dispatch_arms)))),
                            (indent(indent(indent(Line(fmt!(ctx,"_ => Err({capnp}::Error::unimplemented(\"Method not implemented.\".to_string())) ")))))),
                            (indent(line("}")))])
                    } else {
                        indent(indent(indent(line(fmt!(ctx, "Err({capnp}::Error::unimplemented(\"Method not implemented.\".to_string()))")))))
                    }),
                    indent(line("}")),
                    line("}")]));

            private_mod_interior.push(generate_get_params_results(ctx, names, *node_reader)?);
            mod_interior.push(Branch(vec![
                line("mod _private {"),
                indent(private_mod_interior),
                line("}"),
            ]));

            mod_interior.push(Branch(vec![Branch(nested_output)]));

            output.push(BlankLine);
            if is_generic {
                output.push(Line(format!(
                    "pub mod {} {{ /* ({}) */",
                    node_name,
                    params.expanded_list.join(",")
                )));
            } else {
                output.push(Line(format!("pub mod {node_name} {{")));
            }
            output.push(indent(mod_interior));
            output.push(line("}"));
        }

        node::Const(c) => {
            let styled_name = snake_to_upper_case(ctx.get_last_name(node_id)?);

            let typ = c.get_type()?;
            let formatted_text = match (typ.which()?, c.get_value()?.which()?) {
                (type_::Void(()), value::Void(())) => {
                    Line(format!("pub const {styled_name}: () = ();"))
                }
                (type_::Bool(()), value::Bool(b)) => {
                    Line(format!("pub const {styled_name}: bool = {b};"))
                }
                (type_::Int8(()), value::Int8(i)) => {
                    Line(format!("pub const {styled_name}: i8 = {i};"))
                }
                (type_::Int16(()), value::Int16(i)) => {
                    Line(format!("pub const {styled_name}: i16 = {i};"))
                }
                (type_::Int32(()), value::Int32(i)) => {
                    Line(format!("pub const {styled_name}: i32 = {i};"))
                }
                (type_::Int64(()), value::Int64(i)) => {
                    Line(format!("pub const {styled_name}: i64 = {i};"))
                }
                (type_::Uint8(()), value::Uint8(i)) => {
                    Line(format!("pub const {styled_name}: u8 = {i};"))
                }
                (type_::Uint16(()), value::Uint16(i)) => {
                    Line(format!("pub const {styled_name}: u16 = {i};"))
                }
                (type_::Uint32(()), value::Uint32(i)) => {
                    Line(format!("pub const {styled_name}: u32 = {i};"))
                }
                (type_::Uint64(()), value::Uint64(i)) => {
                    Line(format!("pub const {styled_name}: u64 = {i};"))
                }

                (type_::Float32(()), value::Float32(f)) => {
                    Line(format!("pub const {styled_name}: f32 = {f:e}f32;"))
                }

                (type_::Float64(()), value::Float64(f)) => {
                    Line(format!("pub const {styled_name}: f64 = {f:e}f64;"))
                }

                (type_::Enum(e), value::Enum(v)) => {
                    if let Some(node) = ctx.node_map.get(&e.get_type_id()) {
                        match node.which()? {
                            node::Enum(e) => {
                                let enumerants = e.get_enumerants()?;
                                if let Some(enumerant) = enumerants.try_get(u32::from(v)) {
                                    let variant =
                                        capitalize_first_letter(get_enumerant_name(enumerant)?);
                                    let type_string = typ.type_string(ctx, Leaf::Owned)?;
                                    Line(format!(
                                        "pub const {}: {} = {}::{};",
                                        styled_name, &type_string, &type_string, variant
                                    ))
                                } else {
                                    return Err(Error::failed(format!(
                                        "enumerant out of range: {v}"
                                    )));
                                }
                            }
                            _ => {
                                return Err(Error::failed(format!(
                                    "bad enum type ID: {}",
                                    e.get_type_id()
                                )));
                            }
                        }
                    } else {
                        return Err(Error::failed(format!(
                            "bad enum type ID: {}",
                            e.get_type_id()
                        )));
                    }
                }

                (type_::Text(()), value::Text(t)) => Line(format!(
                    "pub const {styled_name}: &str = {:?};",
                    t?.to_str()?
                )),
                (type_::Data(()), value::Data(d)) => {
                    Line(format!("pub const {styled_name}: &[u8] = &{:?};", d?))
                }

                (type_::List(_), value::List(v)) => {
                    generate_pointer_constant(ctx, &styled_name, typ, v)?
                }
                (type_::Struct(_), value::Struct(v)) => {
                    generate_pointer_constant(ctx, &styled_name, typ, v)?
                }

                (type_::Interface(_t), value::Interface(())) => {
                    return Err(Error::unimplemented("interface constants".to_string()));
                }
                (type_::AnyPointer(_), value::AnyPointer(_pr)) => {
                    return Err(Error::unimplemented("anypointer constants".to_string()));
                }

                _ => {
                    return Err(Error::failed("type does not match value".to_string()));
                }
            };

            output.push(formatted_text);
        }

        node::Annotation(annotation_reader) => {
            let is_generic = node_reader.get_is_generic();
            let params = node_reader.parameters_texts(ctx);
            let last_name = ctx.get_last_name(node_id)?;
            let mut interior = vec![];
            interior.push(Line(format!("pub const ID: u64 = 0x{node_id:x};")));

            let ty = annotation_reader.get_type()?;
            if !is_generic {
                interior.push(Line(fmt!(ctx,
                    "pub fn get_type() -> {capnp}::introspect::Type {{ <{} as {capnp}::introspect::Introspect>::introspect() }}", ty.type_string(ctx, Leaf::Owned)?)));
            } else {
                interior.push(Line(fmt!(ctx,"pub fn get_type<{0}>() -> {capnp}::introspect::Type {1} {{ <{2} as {capnp}::introspect::Introspect>::introspect() }}", params.params, params.where_clause, ty.type_string(ctx, Leaf::Owned)?)));
            }
            output.push(Branch(vec![
                Line(format!("pub mod {last_name} {{")),
                indent(interior),
                Line("}".into()),
            ]));
        }
    }

    Ok(Branch(output))
}

// TODO: make indent take Into<FormattedText>, impl Into<FormattedText> for vec (branch)
