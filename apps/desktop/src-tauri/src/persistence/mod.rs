mod database;
mod error;
pub mod sqlite;

pub use database::Database;
pub use error::PersistenceError;
