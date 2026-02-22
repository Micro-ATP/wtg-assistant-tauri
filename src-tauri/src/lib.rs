pub mod commands;
pub mod error;
pub mod models;
pub mod platform;
pub mod services;
pub mod utils;

pub use error::{AppError, Result};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
