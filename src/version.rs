//! Parse PostgreSQL version numbers.
//!
//! ```rust
//! # use postgresfixture::version::Version;
//! assert_eq!(Ok(Version::Pre10(9, 6, 17)), "9.6.17".parse());
//! assert_eq!(Ok(Version::Post10(14, 6)), "14.6".parse());
//! ```
//!
//! See the [PostgreSQL "Versioning Policy" page][versioning] for information on
//! PostgreSQL's versioning scheme.
//!
//! [versioning]: https://www.postgresql.org/support/versioning/

mod current;
mod error;
mod partial;

pub use current::Version;
pub use error::Error;
#[allow(clippy::module_name_repetitions)]
pub use partial::PartialVersion;
