//! OIDC SSO integration (saas mode).
//!
//! Modeled after the canonical pattern in `../rusty-links` and `../dmarc-reporter`,
//! talking OIDC Authorization Code + PKCE to the parent `saas` identity provider.

pub mod jit;
pub mod rp;
pub mod session;
pub mod verifier;

pub use rp::OidcRpState;
#[allow(unused_imports)]
pub use session::{require_session, AuthenticatedUser, RUS_SESSION_COOKIE};
pub use verifier::OidcVerifier;
