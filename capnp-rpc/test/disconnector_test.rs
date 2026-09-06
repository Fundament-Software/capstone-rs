//! Tests that a `Disconnector` future resolves only once the connection's
//! `shutdown()` has completed, and that it propagates shutdown errors.
//! See https://github.com/capnproto/capnproto-rust/issues/583

use std::cell::Cell;
use std::rc::Rc;

use crate::reconnect_test::PollOnce;
use capnp::Error;
use capnp::capability::Promise;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::{Connection, IncomingMessage, OutgoingMessage, RpcSystem, VatNetwork};

use futures_util::FutureExt;
use tokio::sync::oneshot;

struct MockOutgoingMessage {
    message: ::capnp::message::Builder<::capnp::message::HeapAllocator>,
}

impl OutgoingMessage for MockOutgoingMessage {
    fn get_body(&mut self) -> ::capnp::Result<::capnp::any_pointer::Builder<'_>> {
        self.message.get_root()
    }

    fn get_body_as_reader(&self) -> ::capnp::Result<::capnp::any_pointer::Reader<'_>> {
        self.message.get_root_as_reader()
    }

    fn send(
        self: Box<Self>,
    ) -> (
        Promise<(), Error>,
        Rc<::capnp::message::Builder<::capnp::message::HeapAllocator>>,
    ) {
        (Promise::ok(()), Rc::new(self.message))
    }

    fn take(self: Box<Self>) -> ::capnp::message::Builder<::capnp::message::HeapAllocator> {
        self.message
    }

    fn size_in_words(&self) -> usize {
        self.message.size_in_words()
    }
}

/// A connection that never receives any messages and whose `shutdown()`
/// completes with whatever result is sent on the `shutdown_result` channel.
struct MockConnection {
    shutdown_result: Option<oneshot::Receiver<Result<(), Error>>>,
    shutdown_called: Rc<Cell<bool>>,
}

impl Connection<Side> for MockConnection {
    fn get_peer_vat_id(&self) -> Side {
        Side::Server
    }

    fn new_outgoing_message(&mut self, _first_segment_word_size: u32) -> Box<dyn OutgoingMessage> {
        Box::new(MockOutgoingMessage {
            message: ::capnp::message::Builder::new_default(),
        })
    }

    fn receive_incoming_message(&mut self) -> Promise<Option<Box<dyn IncomingMessage>>, Error> {
        Promise::from_future(std::future::pending())
    }

    fn shutdown(&mut self, _result: ::capnp::Result<()>, _flush: bool) -> Promise<(), Error> {
        self.shutdown_called.set(true);
        match self.shutdown_result.take() {
            Some(rx) => Promise::from_future(async move {
                match rx.await {
                    Ok(result) => result,
                    Err(_) => Err(Error::failed("shutdown result sender was dropped".into())),
                }
            }),
            None => Promise::err(Error::failed("shutdown() called twice".into())),
        }
    }
}

struct MockNetwork {
    connection: Option<Box<dyn Connection<Side>>>,
}

impl VatNetwork<Side> for MockNetwork {
    fn connect(&mut self, _host_id: Side) -> Option<Box<dyn Connection<Side>>> {
        self.connection.take()
    }

    fn accept(&mut self) -> Promise<Box<dyn Connection<Side>>, Error> {
        Promise::from_future(std::future::pending())
    }

    fn drive_until_shutdown(&mut self) -> Promise<(), Error> {
        Promise::from_future(std::future::pending())
    }
}

/// A `tokio::task::JoinHandle` that cancels its task on drop, mimicking `futures::future::RemoteHandle`.
struct RemoteHandle<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for RemoteHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> Future for RemoteHandle<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<T> {
        use std::task::Poll;

        match std::pin::Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(Ok(val)) => Poll::Ready(val),
            Poll::Ready(Err(e)) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            Poll::Ready(Err(_)) => panic!("RemoteHandle task was unexpectedly cancelled"),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn mock_setup() -> (
    tokio::task::LocalSet,
    oneshot::Sender<Result<(), Error>>,
    Rc<Cell<bool>>,
    RemoteHandle<Result<(), Error>>,
) {
    let mut pool = tokio::task::LocalSet::new();

    let (tx, rx) = oneshot::channel();
    let shutdown_called = Rc::new(Cell::new(false));
    let network = Box::new(MockNetwork {
        connection: Some(Box::new(MockConnection {
            shutdown_result: Some(rx),
            shutdown_called: shutdown_called.clone(),
        })),
    });

    let mut rpc_system = RpcSystem::new(network, None);
    let disconnector = rpc_system.get_disconnector();

    // Trigger creation of the connection state.
    let _client: crate::test_capnp::bootstrap::Client = rpc_system.bootstrap(Side::Server);

    // The RpcSystem reports the shutdown error too; ignore it so that the
    // spawned task does not panic in the error test.
    let _ = pool.spawn_local(rpc_system.map(|_| ()));

    let disconnector_handle = RemoteHandle(pool.spawn_local(disconnector));

    // Run until all tasks are blocked. The disconnector must not resolve,
    // because the mock shutdown has not completed yet.
    // pool.run_until_stalled(); // tokio doesn't have this so we just poll a bunch

    for _ in 0..63 {
        let _ = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(PollOnce(&mut pool))
        });
    }

    assert!(shutdown_called.get());

    (pool, tx, shutdown_called, disconnector_handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnector_waits_for_connection_shutdown() {
    let (pool, tx, _shutdown_called, mut disconnector_handle) = mock_setup();

    assert!(
        (&mut disconnector_handle).now_or_never().is_none(),
        "disconnector should not resolve before shutdown() completes"
    );

    tx.send(Ok(())).unwrap();
    pool.run_until(disconnector_handle).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnector_propagates_shutdown_error() {
    let (mut pool, tx, _shutdown_called, disconnector_handle) = mock_setup();

    tx.send(Err(Error::failed("mock shutdown failure".into())))
        .unwrap();

    let res = pool.run_until(disconnector_handle).await;

    match res {
        Err(e) => assert!(
            e.to_string().contains("mock shutdown failure"),
            "unexpected error: {e:?}"
        ),
        Ok(()) => panic!("disconnector should have reported the shutdown error"),
    }
}
