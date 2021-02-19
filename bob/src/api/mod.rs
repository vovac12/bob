pub(crate) use super::prelude::*;

#[allow(clippy::needless_pass_by_value)] // allow for http mod, because of rocket lib macro impl
pub mod http;

pub mod prelude {
    pub(crate) use super::*;
    pub(crate) use {
        backend::{Group as PearlGroup, Holder},
        rocket::{
            http::RawStr,
            http::Status,
            request::{FromParam, Request},
            response::{Responder, Response, Result as RocketResult},
            Config, Rocket, State,
        },
        rocket_contrib::json::Json,
        server::Server as BobServer,
    };
}
