pub mod grpc_build {
    pub mod request {
        pub mod helloworld;
    }
    pub mod messages {
        pub mod builtin;
    }
    pub mod client {
        pub mod helloworld;
    }
    pub mod response {
        pub mod helloworld;
    }
}
pub mod google {
    pub mod protobuf;
}
impl grpc_build::messages::builtin::CustomMessage {
    pub fn message_name() -> &'static str {
        "grpc_build.messages.builtin.CustomMessage"
    }
}
impl grpc_build::response::helloworld::HelloReply {
    pub fn message_name() -> &'static str {
        "grpc_build.response.helloworld.HelloReply"
    }
}
impl grpc_build::request::helloworld::HelloRequest {
    pub fn message_name() -> &'static str {
        "grpc_build.request.helloworld.HelloRequest"
    }
}
