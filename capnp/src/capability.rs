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

//! Hooks for for the RPC system.
//!
//! Roughly corresponds to capability.h in the C++ implementation.

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::rc::Rc;
#[cfg(feature = "alloc")]
use core::future::Future;
#[cfg(feature = "alloc")]
use core::marker::{PhantomData, Unpin};
#[cfg(feature = "rpc_try")]
use core::ops::Try;
#[cfg(feature = "alloc")]
use core::pin::Pin;
#[cfg(feature = "alloc")]
use core::task::Poll;

use crate::any_pointer;
#[cfg(feature = "alloc")]
use crate::private::capability::{ClientHook, ParamsHook, RequestHook, ResponseHook, ResultsHook};
#[cfg(feature = "alloc")]
use crate::traits::{Owned, Pipelined};
#[cfg(feature = "alloc")]
use crate::{Error, MessageSize};

/// A computation that might eventually resolve to a value of type `T` or to an error
///  of type `E`. Dropping the promise cancels the computation.
#[cfg(feature = "alloc")]
#[must_use = "futures do nothing unless polled"]
pub struct Promise<T, E> {
    inner: PromiseInner<T, E>,
}

#[cfg(feature = "alloc")]
enum PromiseInner<T, E> {
    Immediate(Result<T, E>),
    Deferred(Pin<Box<dyn Future<Output = core::result::Result<T, E>> + 'static>>),
    Empty,
}

// Allow Promise<T,E> to be Unpin, regardless of whether T and E are.
#[cfg(feature = "alloc")]
impl<T, E> Unpin for PromiseInner<T, E> {}

#[cfg(feature = "alloc")]
impl<T, E> Promise<T, E> {
    pub fn ok(value: T) -> Self {
        Self {
            inner: PromiseInner::Immediate(Ok(value)),
        }
    }

    pub fn err(error: E) -> Self {
        Self {
            inner: PromiseInner::Immediate(Err(error)),
        }
    }

    pub fn from_future<F>(f: F) -> Self
    where
        F: Future<Output = core::result::Result<T, E>> + 'static,
    {
        Self {
            inner: PromiseInner::Deferred(Box::pin(f)),
        }
    }
}

#[cfg(feature = "alloc")]
impl<T, E> Future for Promise<T, E> {
    type Output = core::result::Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut ::core::task::Context) -> Poll<Self::Output> {
        match self.get_mut().inner {
            PromiseInner::Empty => panic!("Promise polled after done."),
            ref mut imm @ PromiseInner::Immediate(_) => {
                match core::mem::replace(imm, PromiseInner::Empty) {
                    PromiseInner::Immediate(r) => Poll::Ready(r),
                    _ => unreachable!(),
                }
            }
            PromiseInner::Deferred(ref mut f) => f.as_mut().poll(cx),
        }
    }
}

//Minimal version of futures::future::Either for dispatch_call_internal() without allocating a Box
#[cfg(feature = "alloc")]
pub enum Either<A, B> {
    A(A),
    B(B),
}
#[cfg(feature = "alloc")]
impl<A, B> Future for Either<A, B>
where
    A: Future,
    B: Future<Output = A::Output>,
{
    type Output = A::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> Poll<Self::Output> {
        match self.as_pin_mut() {
            Either::A(x) => x.poll(cx),
            Either::B(x) => x.poll(cx),
        }
    }
}
#[cfg(feature = "alloc")]
impl<A, B> Either<A, B> {
    pub fn as_pin_mut(self: Pin<&mut Self>) -> Either<Pin<&mut A>, Pin<&mut B>> {
        unsafe {
            match *Pin::get_unchecked_mut(self) {
                Either::A(ref mut inner) => Either::A(Pin::new_unchecked(inner)),
                Either::B(ref mut inner) => Either::B(Pin::new_unchecked(inner)),
            }
        }
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "rpc_try")]
impl<T> core::ops::Try for Promise<T, crate::Error> {
    type Output = Self;
    type Residual = Result<core::convert::Infallible, crate::Error>;

    fn from_output(output: Self::Output) -> Self {
        output
    }

    fn branch(self) -> core::ops::ControlFlow<Self::Residual, Self::Output> {
        unimplemented!();
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "rpc_try")]
impl<T> core::ops::FromResidual for Promise<T, crate::Error> {
    fn from_residual(residual: <Self as Try>::Residual) -> Self {
        match residual {
            Ok(_) => unimplemented!(),
            Err(e) => Self::err(e),
        }
    }
}

/// A promise for a result from a method call.
#[cfg(feature = "alloc")]
#[must_use]
pub struct RemotePromise<Results>
where
    Results: Pipelined + Owned + 'static,
{
    pub promise: Promise<Response<Results>, crate::Error>,
    pub pipeline: Results::Pipeline,
}

/// A response from a method call, as seen by the client.
#[cfg(feature = "alloc")]
pub struct Response<Results> {
    pub marker: PhantomData<Results>,
    pub hook: Box<dyn ResponseHook>,
}

#[cfg(feature = "alloc")]
impl<Results> Response<Results>
where
    Results: Pipelined + Owned,
{
    pub fn new(hook: Box<dyn ResponseHook>) -> Self {
        Self {
            marker: PhantomData,
            hook,
        }
    }
    pub fn get(&self) -> crate::Result<Results::Reader<'_>> {
        self.hook.get()?.get_as()
    }
}

/// A method call that has not been sent yet.
#[cfg(feature = "alloc")]
pub struct Request<Params, Results> {
    pub marker: PhantomData<(Params, Results)>,
    pub hook: Box<dyn RequestHook>,
}

#[cfg(feature = "alloc")]
impl<Params, Results> Request<Params, Results>
where
    Params: Owned,
{
    pub fn new(hook: Box<dyn RequestHook>) -> Self {
        Self {
            hook,
            marker: PhantomData,
        }
    }

    pub fn get(&mut self) -> Params::Builder<'_> {
        self.hook.get().get_as().unwrap()
    }

    pub fn set(&mut self, from: Params::Reader<'_>) -> crate::Result<()> {
        self.hook.get().set_as(from)
    }
}

#[cfg(feature = "alloc")]
impl<Params, Results> Request<Params, Results>
where
    Results: Pipelined + Owned + 'static + Unpin,
    <Results as Pipelined>::Pipeline: FromTypelessPipeline,
{
    pub fn send(self) -> RemotePromise<Results> {
        let RemotePromise {
            promise, pipeline, ..
        } = self.hook.send();
        let typed_promise = Promise::from_future(async move {
            Ok(Response {
                hook: promise.await?.hook,
                marker: PhantomData,
            })
        });
        RemotePromise {
            promise: typed_promise,
            pipeline: FromTypelessPipeline::new(pipeline),
        }
    }
}

/// The values of the parameters passed to a method call, as seen by the server.
#[cfg(feature = "alloc")]
pub struct Params<T> {
    pub marker: PhantomData<T>,
    pub hook: Box<dyn ParamsHook>,
}

#[cfg(feature = "alloc")]
impl<T> Params<T> {
    pub fn new(hook: Box<dyn ParamsHook>) -> Self {
        Self {
            marker: PhantomData,
            hook,
        }
    }
    pub fn get(&self) -> crate::Result<T::Reader<'_>>
    where
        T: Owned,
    {
        self.hook.get()?.get_as()
    }
}

/// The return values of a method, written in-place by the method body.
#[cfg(feature = "alloc")]
pub struct Results<T> {
    pub marker: PhantomData<T>,
    pub hook: Box<dyn ResultsHook>,
}

#[cfg(feature = "alloc")]
impl<T> Results<T>
where
    T: Owned,
{
    pub fn new(hook: Box<dyn ResultsHook>) -> Self {
        Self {
            marker: PhantomData,
            hook,
        }
    }

    pub fn get(&mut self) -> T::Builder<'_> {
        self.hook.get().unwrap().get_as().unwrap()
    }

    pub fn set(&mut self, other: T::Reader<'_>) -> crate::Result<()> {
        self.hook.get().unwrap().set_as(other)
    }
}

pub trait FromTypelessPipeline {
    fn new(typeless: any_pointer::Pipeline) -> Self;
}

/// Trait implemented (via codegen) by all user-defined capability client types.
#[cfg(feature = "alloc")]
pub trait FromClientHook: crate::introspect::Introspect {
    /// Wraps a client hook to create a new client.
    fn new(hook: Box<dyn ClientHook>) -> Self;

    /// Unwraps client to get the underlying client hook.
    fn into_client_hook(self) -> Box<dyn ClientHook>;

    /// Gets a reference to the underlying client hook.
    fn as_client_hook(&self) -> &dyn ClientHook;

    /// Casts `self` to another instance of `FromClientHook`. This always succeeds,
    /// but if the underlying capability does not actually implement `T`'s interface,
    /// then method calls will fail with "unimplemented" errors.
    fn cast_to<T: FromClientHook + Sized>(self) -> T
    where
        Self: Sized,
    {
        FromClientHook::new(self.into_client_hook())
    }
}

/// An untyped client.
#[cfg(feature = "alloc")]
#[derive(Clone)]
pub struct Client {
    pub hook: Box<dyn ClientHook>,
}

#[cfg(feature = "alloc")]
impl Client {
    pub fn new(hook: Box<dyn ClientHook>) -> Self {
        Self { hook }
    }

    pub fn new_call<Params, Results>(
        &self,
        interface_id: u64,
        method_id: u16,
        size_hint: Option<MessageSize>,
    ) -> Request<Params, Results> {
        let typeless = self.hook.new_call(interface_id, method_id, size_hint);
        Request {
            hook: typeless.hook,
            marker: PhantomData,
        }
    }

    /// If the capability is actually only a promise, the returned promise resolves once the
    /// capability itself has resolved to its final destination (or propagates the exception if
    /// the capability promise is rejected).  This is mainly useful for error-checking in the case
    /// where no calls are being made.  There is no reason to wait for this before making calls; if
    /// the capability does not resolve, the call results will propagate the error.
    pub fn when_resolved(&self) -> Promise<(), Error> {
        self.hook.when_resolved()
    }
}

#[cfg(feature = "alloc")]
// This is an untyped dispatch for an untyped server, which forwards calls directly to dispatch_call
pub struct UntypedDispatch<_T> {
    pub server: Rc<_T>,
}

#[cfg(feature = "alloc")]
impl<_T> Clone for UntypedDispatch<_T> {
    fn clone(&self) -> Self {
        Self {
            server: self.server.clone(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<_T: Server> ::core::ops::Deref for UntypedDispatch<_T> {
    type Target = _T;
    fn deref(&self) -> &_T {
        self.server.as_ref()
    }
}

#[cfg(feature = "alloc")]
impl<_T: Server + Clone> crate::capability::Server for UntypedDispatch<_T> {
    async fn dispatch_call(
        self,
        interface_id: u64,
        method_id: u16,
        params: crate::capability::Params<any_pointer::Owned>,
        results: crate::capability::Results<any_pointer::Owned>,
    ) -> Result<(), crate::Error> {
        <_T as Clone>::clone(&self.server)
            .dispatch_call(interface_id, method_id, params, results)
            .await
    }
    fn get_ptr(&self) -> usize {
        Rc::<_T>::as_ptr(&self.server) as usize
    }
}

#[cfg(feature = "alloc")]
impl crate::introspect::Introspect for Client {
    fn introspect() -> crate::introspect::Type {
        crate::introspect::TypeVariant::Capability(crate::introspect::RawCapabilitySchema {
            encoded_node: &[],
            params_types: crate::schema::dynamic_struct_marker,
            result_types: crate::schema::dynamic_struct_marker,
        })
        .into()
    }
}

#[cfg(feature = "alloc")]
impl<_S: Server + 'static + Clone> crate::capability::FromServer<_S> for Client {
    type Dispatch = UntypedDispatch<_S>;
    fn from_server(s: _S) -> UntypedDispatch<_S> {
        UntypedDispatch { server: Rc::new(s) }
    }
    fn from_rc(s: Rc<_S>) -> UntypedDispatch<_S> {
        UntypedDispatch { server: s }
    }
}

#[cfg(feature = "alloc")]
impl crate::capability::FromClientHook for Client {
    fn new(hook: Box<dyn (ClientHook)>) -> Self {
        Self { hook }
    }
    fn into_client_hook(self) -> Box<dyn (ClientHook)> {
        self.hook
    }
    fn as_client_hook(&self) -> &dyn (ClientHook) {
        &*self.hook
    }
}

/// An untyped server.
#[allow(async_fn_in_trait)]
#[cfg(feature = "alloc")]
pub trait Server {
    async fn dispatch_call(
        self,
        interface_id: u64,
        method_id: u16,
        params: Params<any_pointer::Owned>,
        results: Results<any_pointer::Owned>,
    ) -> Result<(), Error>;
    fn get_ptr(&self) -> usize;
}

/// Trait to track the relationship between generated Server traits and Client structs.
#[cfg(feature = "alloc")]
pub trait FromServer<S>: FromClientHook {
    // Implemented by the generated ServerDispatch struct.
    type Dispatch: Server + 'static + Clone;

    fn from_server(s: S) -> Self::Dispatch;
    fn from_rc(s: Rc<S>) -> Self::Dispatch;
}

/// Gets the "resolved" version of a capability. One place this is useful is for pre-resolving
/// the argument to `capnp_rpc::CapabilityServerSet::get_local_server_of_resolved()`.
#[cfg(feature = "alloc")]
pub async fn get_resolved_cap<C: FromClientHook>(cap: C) -> C {
    let mut hook = cap.into_client_hook();
    let _ = hook.when_resolved().await;
    while let Some(resolved) = hook.get_resolved() {
        hook = resolved;
    }
    FromClientHook::new(hook)
}

#[cfg(feature = "alloc")]
impl core::fmt::Debug for dyn ClientHook {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(
            "Cap index: {} (Brand: {})",
            self.get_ptr(),
            self.get_brand(),
        ))
    }
}
