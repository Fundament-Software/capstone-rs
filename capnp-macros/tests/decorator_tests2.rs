pub mod test_schema_capnp {
    include!(concat!(env!("OUT_DIR"), "/test_schema_capnp.rs"));
}

use std::rc::Rc;

use capnp_macros::capnproto_rpc;
use test_schema_capnp::generic_interface;

#[allow(dead_code)]
#[derive(Default)]
struct GenericInterfaceImpl<T> {
    value: T,
}

type T0 = capnp::text::Owned;

#[capnproto_rpc(generic_interface)]
impl generic_interface::Server<T0> for GenericInterfaceImpl<T0> {
    #[allow(unused_variables)]
    async fn generic_set_value(
        self: Rc<Self>,
        value: GenericInterfaceImpl<T0>,
    ) -> Result<(), capnp::Error> {
        Ok(())
    }

    async fn generic_get_value(self: Rc<Self>) {
        Ok(())
    }
}

// Mostly to show that it compiles, I'm not sure how to instantiate
#[tokio::test]
async fn decorator_generic_test() -> capnp::Result<()> {
    let _client: generic_interface::Client<T0>;
    Ok(())
}
