use rocket::response::{self, Responder};
use rocket::Request;

pub type Result<T = ()> = std::result::Result<T, Error>;

/// Wrapper around [`anyhow::Error`]
/// with rocket's [responder] implemented
///
/// [anyhow::Error]: https://docs.rs/anyhow/1.0/anyhow/struct.Error.html
/// [responder]: https://api.rocket.rs/v0.4/rocket/response/trait.Responder.html
/// Error that can be convert into `anyhow::Error` can be convert directly to this type.
///
/// Responder part are internally delegated to [rocket::response::Debug] which
/// "debug prints the internal value before responding with a 500 error"
///
/// [rocket::response::Debug]: https://api.rocket.rs/v0.4/rocket/response/struct.Debug.html
#[derive(Debug)]
pub struct Error(pub anyhow::Error);

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Error(error.into())
    }
}

impl<'r> Responder<'r, 'static> for Error {
    fn respond_to(self, _request: &'r Request<'_>) -> response::Result<'static> {
        use std::io::Cursor;

        use rocket::http::ContentType;
        use rocket::response::Response;
        // response::Debug(self.0).respond_to(request)
        let err_str = format!("http 500 \n\n{:?}", self.0);
        Response::build()
            .header(ContentType::Plain)
            .sized_body(err_str.len(), Cursor::new(err_str))
            .status(rocket::http::Status::InternalServerError)
            .ok()
    }
}

#[allow(unused_macros)]
macro_rules! bail {
    ($msg:literal $(,)?) => {
        return Err(rocket_anyhow::Error(anyhow::anyhow!($msg)))
    };
    ($err:expr $(,)?) => {
        return Err(rocket_anyhow::Error(anyhow::anyhow!($err)))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err(rocket_anyhow::Error(anyhow::anyhow!($fmt, $($arg)*)))
    };
}

#[derive(Debug)]
pub struct Debug<E>(pub E);

impl<E> From<E> for Debug<E> {
    #[inline(always)]
    fn from(e: E) -> Self {
        Debug(e)
    }
}
