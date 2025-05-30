// Copyright (c) 2013-2017 Sandstorm Development Group, Inc. and contributors
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

use capnp::Error;
use capnp::capability::{self, Promise};
use capnp::private::capability::{
    ClientHook, ParamsHook, PipelineHook, PipelineOp, RequestHook, ResponseHook, ResultsHook,
};
use capnp::traits::{Imbue, ImbueMut};
use capnp::{any_pointer, message};

use futures_util::TryFutureExt;
use tokio::sync::oneshot;

use std::cell::RefCell;
use std::rc::Rc;

pub trait ResultsDoneHook {
    fn add_ref(&self) -> Box<dyn ResultsDoneHook>;
    fn get(&self) -> ::capnp::Result<any_pointer::Reader>;
}

impl Clone for Box<dyn ResultsDoneHook> {
    fn clone(&self) -> Self {
        self.add_ref()
    }
}

pub struct Response {
    results: Box<dyn ResultsDoneHook>,
}

impl Response {
    fn new(results: Box<dyn ResultsDoneHook>) -> Self {
        Self { results }
    }
}

impl ResponseHook for Response {
    fn get(&self) -> ::capnp::Result<any_pointer::Reader> {
        self.results.get()
    }
}

struct Params {
    request: message::Builder<message::HeapAllocator>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
}

impl Params {
    fn new(
        request: message::Builder<message::HeapAllocator>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
    ) -> Self {
        Self { request, cap_table }
    }
}

impl ParamsHook for Params {
    fn get(&self) -> ::capnp::Result<any_pointer::Reader> {
        let mut result: any_pointer::Reader = self.request.get_root_as_reader()?;
        result.imbue(&self.cap_table);
        Ok(result)
    }
}

struct Results {
    message: Option<message::Builder<message::HeapAllocator>>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
    results_done_fulfiller: Option<oneshot::Sender<Box<dyn ResultsDoneHook>>>,
}

impl Results {
    fn new(fulfiller: oneshot::Sender<Box<dyn ResultsDoneHook>>) -> Self {
        Self {
            message: Some(::capnp::message::Builder::new_default()),
            cap_table: Vec::new(),
            results_done_fulfiller: Some(fulfiller),
        }
    }
}

impl Drop for Results {
    fn drop(&mut self) {
        if let (Some(message), Some(fulfiller)) =
            (self.message.take(), self.results_done_fulfiller.take())
        {
            let cap_table = ::std::mem::take(&mut self.cap_table);
            let _ = fulfiller.send(Box::new(ResultsDone::new(message, cap_table)));
        } else {
            unreachable!()
        }
    }
}

impl ResultsHook for Results {
    fn get(&mut self) -> ::capnp::Result<any_pointer::Builder> {
        match *self {
            Self {
                message: Some(ref mut message),
                ref mut cap_table,
                ..
            } => {
                let mut result: any_pointer::Builder = message.get_root()?;
                result.imbue_mut(cap_table);
                Ok(result)
            }
            _ => unreachable!(),
        }
    }

    fn tail_call(self: Box<Self>, _request: Box<dyn RequestHook>) -> Promise<(), Error> {
        unimplemented!()
    }

    fn direct_tail_call(
        self: Box<Self>,
        _request: Box<dyn RequestHook>,
    ) -> (Promise<(), Error>, Box<dyn PipelineHook>) {
        unimplemented!()
    }

    fn allow_cancellation(&self) {
        unimplemented!()
    }
}

struct ResultsDoneInner {
    message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
}

struct ResultsDone {
    inner: Rc<ResultsDoneInner>,
}

impl ResultsDone {
    fn new(
        message: message::Builder<message::HeapAllocator>,
        cap_table: Vec<Option<Box<dyn ClientHook>>>,
    ) -> Self {
        Self {
            inner: Rc::new(ResultsDoneInner { message, cap_table }),
        }
    }
}

impl ResultsDoneHook for ResultsDone {
    fn add_ref(&self) -> Box<dyn ResultsDoneHook> {
        Box::new(Self {
            inner: self.inner.clone(),
        })
    }
    fn get(&self) -> ::capnp::Result<any_pointer::Reader> {
        let mut result: any_pointer::Reader = self.inner.message.get_root_as_reader()?;
        result.imbue(&self.inner.cap_table);
        Ok(result)
    }
}

pub struct Request {
    message: message::Builder<::capnp::message::HeapAllocator>,
    cap_table: Vec<Option<Box<dyn ClientHook>>>,
    interface_id: u64,
    method_id: u16,
    client: Box<dyn ClientHook>,
}

impl Request {
    pub fn new(
        interface_id: u64,
        method_id: u16,
        _size_hint: Option<::capnp::MessageSize>,
        client: Box<dyn ClientHook>,
    ) -> Self {
        Self {
            message: message::Builder::new_default(),
            cap_table: Vec::new(),
            interface_id,
            method_id,
            client,
        }
    }
}

impl RequestHook for Request {
    fn get(&mut self) -> any_pointer::Builder {
        let mut result: any_pointer::Builder = self.message.get_root().unwrap();
        result.imbue_mut(&mut self.cap_table);
        result
    }
    fn get_brand(&self) -> usize {
        0
    }
    fn send(self: Box<Self>) -> capability::RemotePromise<any_pointer::Owned> {
        let tmp = *self;
        let Self {
            message,
            cap_table,
            interface_id,
            method_id,
            client,
        } = tmp;
        let params = Params::new(message, cap_table);

        let (results_done_fulfiller, results_done_promise) =
            oneshot::channel::<Box<dyn ResultsDoneHook>>();
        let results_done_promise = results_done_promise.map_err(crate::canceled_to_error);
        let results = Results::new(results_done_fulfiller);
        let promise = client.call(interface_id, method_id, Box::new(params), Box::new(results));

        let (pipeline_sender, mut pipeline) = crate::queued::Pipeline::new();

        let p = futures_util::future::try_join(promise, results_done_promise).and_then(
            move |((), results_done_hook)| {
                pipeline_sender
                    .complete(Box::new(Pipeline::new(results_done_hook.add_ref()))
                        as Box<dyn PipelineHook>);
                Promise::ok((
                    capability::Response::new(Box::new(Response::new(results_done_hook))),
                    (),
                ))
            },
        );

        let (left, right) = crate::split::split(p);

        pipeline.drive(right);
        let pipeline = any_pointer::Pipeline::new(Box::new(pipeline));

        capability::RemotePromise {
            promise: Promise::from_future(left),
            pipeline,
        }
    }
    fn tail_send(self: Box<Self>) -> Option<(u32, Promise<(), Error>, Box<dyn PipelineHook>)> {
        unimplemented!()
    }
}

struct PipelineInner {
    results: Box<dyn ResultsDoneHook>,
}

pub struct Pipeline {
    inner: Rc<RefCell<PipelineInner>>,
}

impl Pipeline {
    pub fn new(results: Box<dyn ResultsDoneHook>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(PipelineInner { results })),
        }
    }
}

impl Clone for Pipeline {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl PipelineHook for Pipeline {
    fn add_ref(&self) -> Box<dyn PipelineHook> {
        Box::new(self.clone())
    }
    fn get_pipelined_cap(&self, ops: &[PipelineOp]) -> Box<dyn ClientHook> {
        match self
            .inner
            .borrow_mut()
            .results
            .get()
            .unwrap()
            .get_pipelined_cap(ops)
        {
            Ok(v) => v,
            Err(e) => Box::new(crate::broken::Client::new(e, true, 0)) as Box<dyn ClientHook>,
        }
    }
}

pub struct Client<S>
where
    S: capability::Server + Clone,
{
    inner: S,
}

impl<S> Client<S>
where
    S: capability::Server + Clone,
{
    pub fn new(server: S) -> Self {
        Self { inner: server }
    }
}

impl<S> Clone for Client<S>
where
    S: capability::Server + Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<S> ClientHook for Client<S>
where
    S: capability::Server + 'static + Clone,
{
    fn add_ref(&self) -> Box<dyn ClientHook> {
        Box::new(self.clone())
    }
    fn new_call(
        &self,
        interface_id: u64,
        method_id: u16,
        size_hint: Option<::capnp::MessageSize>,
    ) -> capability::Request<any_pointer::Owned, any_pointer::Owned> {
        capability::Request::new(Box::new(Request::new(
            interface_id,
            method_id,
            size_hint,
            self.add_ref(),
        )))
    }

    fn call(
        &self,
        interface_id: u64,
        method_id: u16,
        params: Box<dyn ParamsHook>,
        results: Box<dyn ResultsHook>,
    ) -> Promise<(), Error> {
        // We don't want to actually dispatch the call synchronously, because we don't want the callee
        // to have any side effects before the promise is returned to the caller.  This helps avoid
        // race conditions.
        //
        // TODO: actually use some kind of queue here to guarantee that call order in maintained.
        // This currently relies on the task scheduler being first-in-first-out.
        let inner = self.inner.clone();
        Promise::from_future(async move {
            inner
                .dispatch_call(
                    interface_id,
                    method_id,
                    ::capnp::capability::Params::new(params),
                    ::capnp::capability::Results::new(results),
                )
                .await
        })
    }

    fn get_ptr(&self) -> usize {
        self.inner.get_ptr()
    }

    fn get_brand(&self) -> usize {
        0
    }

    fn get_resolved(&self) -> Option<Box<dyn ClientHook>> {
        None
    }

    fn when_more_resolved(&self) -> Option<Promise<Box<dyn ClientHook>, Error>> {
        None
    }

    fn when_resolved(&self) -> Promise<(), Error> {
        crate::rpc::default_when_resolved_impl(self)
    }

    fn is_local_client(&self) -> bool {
        true
    }
}
