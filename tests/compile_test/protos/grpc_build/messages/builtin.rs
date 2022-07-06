#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CustomMessage {
    #[prost(message, optional, tag = "8")]
    pub timestamp: ::core::option::Option<::prost_types::Timestamp>,
    #[prost(message, optional, tag = "5")]
    pub string_value: ::core::option::Option<::prost::alloc::string::String>,
}
