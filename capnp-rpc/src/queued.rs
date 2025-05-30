// Copyright (c) 2013-2016 Sandstorm Development Group, Inc. and contributors
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

use capnp::Error;
use capnp::any_pointer;
use capnp::capability::Promise;
use capnp::private::capability::{ClientHook, ParamsHook, PipelineHook, PipelineOp, ResultsHook};

use futures_util::{FutureExt, TryFutureExt};
use std::cell::RefCell;
use std::future::Future;
use std::rc::{Rc, Weak};

use crate::attach::Attach;
use crate::sender_queue::SenderQueue;
use crate::{broken, local};

pub struct PipelineInner {
    // Once the promise resolves, this will become non-null and point to the underlying object.
    redirect: Option<Box<dyn PipelineHook>>,

    promise_to_drive: futures_util::future::Shared<Promise<(), Error>>,

    clients_to_resolve: SenderQueue<(Weak<RefCell<ClientInner>>, Vec<PipelineOp>), ()>,
}

impl PipelineInner {
    fn resolve(this: &Rc<RefCell<Self>>, result: Result<Box<dyn PipelineHook>, Error>) {
        assert!(this.borrow().redirect.is_none());
        let pipeline = match result {
            Ok(pipeline_hook) => pipeline_hook,
            Err(e) => Box::new(broken::Pipeline::new(e)),
        };

        this.borrow_mut().redirect = Some(pipeline.add_ref());

        for ((weak_client, ops), waiter) in this.borrow_mut().clients_to_resolve.drain() {
            if let Some(client) = weak_client.upgrade() {
                let clienthook = pipeline.get_pipelined_cap_move(ops);
                ClientInner::resolve(&client, Ok(clienthook));
            }
            let _ = waiter.send(());
        }

        this.borrow_mut().promise_to_drive = Promise::ok(()).shared();
    }
}

pub struct PipelineInnerSender {
    inner: Option<Weak<RefCell<PipelineInner>>>,
}

impl Drop for PipelineInnerSender {
    fn drop(&mut self) {
        if let Some(weak_queued) = self.inner.take() {
            if let Some(pipeline_inner) = weak_queued.upgrade() {
                PipelineInner::resolve(
                    &pipeline_inner,
                    Ok(Box::new(crate::broken::Pipeline::new(Error::failed(
                        "PipelineInnerSender was canceled".into(),
                    )))),
                );
            }
        }
    }
}

impl PipelineInnerSender {
    pub fn complete(mut self, pipeline: Box<dyn PipelineHook>) {
        if let Some(weak_queued) = self.inner.take() {
            if let Some(pipeline_inner) = weak_queued.upgrade() {
                crate::queued::PipelineInner::resolve(&pipeline_inner, Ok(pipeline));
            }
        }
    }
}

pub struct Pipeline {
    inner: Rc<RefCell<PipelineInner>>,
}

impl Pipeline {
    pub fn new() -> (PipelineInnerSender, Self) {
        let inner = Rc::new(RefCell::new(PipelineInner {
            redirect: None,
            promise_to_drive: Promise::ok(()).shared(),
            clients_to_resolve: SenderQueue::new(),
        }));

        (
            PipelineInnerSender {
                inner: Some(Rc::downgrade(&inner)),
            },
            Self { inner },
        )
    }

    pub fn drive<F>(&mut self, promise: F)
    where
        F: Future<Output = Result<(), Error>> + 'static + Unpin,
    {
        let new = Promise::from_future(
            futures_util::future::try_join(
                self.inner.borrow_mut().promise_to_drive.clone(),
                promise,
            )
            .map_ok(|_| ()),
        )
        .shared();
        self.inner.borrow_mut().promise_to_drive = new;
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
        self.get_pipelined_cap_move(ops.into())
    }

    fn get_pipelined_cap_move(&self, ops: Vec<PipelineOp>) -> Box<dyn ClientHook> {
        if let Some(p) = &self.inner.borrow().redirect {
            return p.get_pipelined_cap_move(ops);
        }

        let mut queued_client = Client::new(Some(self.inner.clone()));
        queued_client.drive(self.inner.borrow().promise_to_drive.clone());
        let weak_queued = Rc::downgrade(&queued_client.inner);
        self.inner
            .borrow_mut()
            .clients_to_resolve
            .push_detach((weak_queued, ops));

        Box::new(queued_client)
    }
}

#[allow(clippy::type_complexity)]
pub struct ClientInner {
    // Once the promise resolves, this will become non-null and point to the underlying object.
    redirect: Option<Box<dyn ClientHook>>,

    // The queued::PipelineInner that this client is derived from, if any. We need to hold on
    // to a reference to it so that it doesn't get canceled before the client is resolved.
    pipeline_inner: Option<Rc<RefCell<PipelineInner>>>,

    promise_to_drive: Option<futures_util::future::Shared<Promise<(), Error>>>,

    // When this promise resolves, each queued call will be forwarded to the real client.  This needs
    // to occur *before* any 'whenMoreResolved()' promises resolve, because we want to make sure
    // previously-queued calls are delivered before any new calls made in response to the resolution.
    call_forwarding_queue:
        SenderQueue<(u64, u16, Box<dyn ParamsHook>, Box<dyn ResultsHook>), Promise<(), Error>>,

    // whenMoreResolved() returns forks of this promise.  These must resolve *after* queued calls
    // have been initiated (so that any calls made in the whenMoreResolved() handler are correctly
    // delivered after calls made earlier), but *before* any queued calls return (because it might
    // confuse the application if a queued call returns before the capability on which it was made
    // resolves).  Luckily, we know that queued calls will involve, at the very least, an
    // eventLoop.evalLater.
    client_resolution_queue: SenderQueue<(), Box<dyn ClientHook>>,
}

impl ClientInner {
    pub fn resolve(state: &Rc<RefCell<Self>>, result: Result<Box<dyn ClientHook>, Error>) {
        assert!(state.borrow().redirect.is_none());
        let client = match result {
            Ok(clienthook) => clienthook,
            Err(e) => broken::new_cap(e),
        };
        state.borrow_mut().redirect = Some(client.add_ref());
        for (args, waiter) in state.borrow_mut().call_forwarding_queue.drain() {
            let (interface_id, method_id, params, results) = args;
            let result_promise = client.call(interface_id, method_id, params, results);
            let _ = waiter.send(result_promise);
        }

        for ((), waiter) in state.borrow_mut().client_resolution_queue.drain() {
            let _ = waiter.send(client.add_ref());
        }
        state.borrow_mut().promise_to_drive.take();
        state.borrow_mut().pipeline_inner.take();
    }
}

pub struct Client {
    pub inner: Rc<RefCell<ClientInner>>,
}

impl Client {
    pub fn new(pipeline_inner: Option<Rc<RefCell<PipelineInner>>>) -> Self {
        let inner = Rc::new(RefCell::new(ClientInner {
            promise_to_drive: None,
            pipeline_inner,
            redirect: None,
            call_forwarding_queue: SenderQueue::new(),
            client_resolution_queue: SenderQueue::new(),
        }));
        Self { inner }
    }

    pub fn drive<F>(&mut self, promise: F)
    where
        F: Future<Output = Result<(), Error>> + 'static + Unpin,
    {
        assert!(self.inner.borrow().promise_to_drive.is_none());
        self.inner.borrow_mut().promise_to_drive = Some(Promise::from_future(promise).shared());
    }
}

impl ClientHook for Client {
    fn add_ref(&self) -> Box<dyn ClientHook> {
        Box::new(Self {
            inner: self.inner.clone(),
        })
    }
    fn new_call(
        &self,
        interface_id: u64,
        method_id: u16,
        size_hint: Option<::capnp::MessageSize>,
    ) -> ::capnp::capability::Request<any_pointer::Owned, any_pointer::Owned> {
        ::capnp::capability::Request::new(Box::new(local::Request::new(
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
        if let Some(client) = &self.inner.borrow().redirect {
            return client.call(interface_id, method_id, params, results);
        }

        let inner_clone = self.inner.clone();
        let promise = self
            .inner
            .borrow_mut()
            .call_forwarding_queue
            .push((interface_id, method_id, params, results))
            .attach(inner_clone)
            .and_then(|x| x);

        match self.inner.borrow().promise_to_drive {
            Some(ref p) => Promise::from_future(
                futures_util::future::try_join(p.clone(), promise).map_ok(|v| v.1),
            ),
            None => Promise::from_future(promise),
        }
    }

    fn get_ptr(&self) -> usize {
        (&*self.inner.borrow()) as *const _ as usize
    }

    fn get_brand(&self) -> usize {
        0
    }

    fn get_resolved(&self) -> Option<Box<dyn ClientHook>> {
        match &self.inner.borrow().redirect {
            Some(inner) => Some(inner.clone()),
            None => None,
        }
    }

    fn when_more_resolved(&self) -> Option<Promise<Box<dyn ClientHook>, Error>> {
        if let Some(client) = &self.inner.borrow().redirect {
            return Some(Promise::ok(client.add_ref()));
        }

        let promise = self.inner.borrow_mut().client_resolution_queue.push(());
        match &self.inner.borrow().promise_to_drive {
            Some(p) => Some(Promise::from_future(
                futures_util::future::try_join(p.clone(), promise).map_ok(|v| v.1),
            )),
            None => Some(Promise::from_future(promise)),
        }
    }

    fn when_resolved(&self) -> Promise<(), Error> {
        crate::rpc::default_when_resolved_impl(self)
    }

    fn is_local_client(&self) -> bool {
        false
    }
}
