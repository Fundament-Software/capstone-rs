pub mod test_schema_capnp {
    include!(concat!(env!("OUT_DIR"), "/test_schema_capnp.rs"));
}

use std::rc::Rc;

use capnp_macros::{capnp_build, capnproto_rpc};
use test_schema_capnp::test_interface;

// struct LogSinkImpl;

// impl log_sink::Server for LogSinkImpl {
//     #[capnproto_rpc(log_sink)]
//     fn log(&mut self, id: u64, machine_id: u64, schema: u64, data: String) -> log_sink::LogResults {
//         self.append(id, machine_id, schema, data);
//     }

//     // Should get transformed into:
//     // fn log(
//     //         &mut self,
//     //         params: log_sink::LogParams,
//     //         result: log_sink::LogResults,
//     //     ) -> ::capnp::capability::Promise<(), ::capnp::Error> {
//     //         let rparams = pry!(params.get());
//     //         capnp_let!(
//     //           {id, machineId, schema, data} = rparams
//     //         );
//     //         self.append(id, machineId, schema, data);
//     //     }
// }

#[derive(Default)]
struct TestInterfaceImpl {
    value: std::cell::RefCell<u64>,
}

#[capnproto_rpc(test_interface)]
impl test_interface::Server for TestInterfaceImpl {
    async fn set_value(self: Rc<Self>, value: u64) {
        *self.value.borrow_mut() = value;
        Ok(())
    }

    async fn get_value(self: Rc<Self>) {
        let mut rresult = results.get();
        capnp_build!(rresult, { value = self.value.borrow().clone() });
        Ok(())
    }
}

#[tokio::test]
async fn decorator_test() -> capnp::Result<()> {
    let client: test_interface::Client =
        capnp_rpc::new_client::<_, TestInterfaceImpl>(Default::default());

    // Setting value
    let mut request = client.set_value_request();
    {
        request.get().set_value(3);
    }
    request.send().promise.await?;

    let request = client.get_value_request();
    let response = request.send().promise.await?;
    let response = response.get()?.get_value();
    assert_eq!(response, 3);
    Ok(())
}
