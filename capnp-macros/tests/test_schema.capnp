@0xb829023fa4c284aa;  #unique file ID, generated by `capnp id`

struct TestStruct {
  textField @0 :Text;
  uintField @1 :UInt8;
  voidField @2 :Void;
  boolField @3 :Bool;
  floatField @4 :Float32;
  dataField @5 :Data;
  intlistField @6 :List(Int8);
  structField @7 :TestStruct;
}

interface TestInterface {
  setValue @0 (value :UInt64);
  getValue @1 () -> (value :UInt64);
}

interface GenericInterface(T) {
  genericSetValue @0 (value :T);
  genericGetValue @1 () -> (value :T);
}
