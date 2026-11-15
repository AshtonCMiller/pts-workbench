//! Ford PTS for PTS Workbench: asked directly with the profile's Ford sign-in (its cookies), the way Ford's own pages
//! ask. To be ported from the Electron app's `src/manual.js`, one module per part:
//!
//! | module (to come)    | what                                                                  |
//! |---------------------|-----------------------------------------------------------------------|
//! | `vehicle`           | opening a VIN in PTS (the session's vehicle), its book list           |
//! | `workshop`          | the Workshop Manual and PC/ED: contents, search, procedures, pinpoints |
//! | `wiring`            | wiring diagrams: contents, pages, connector views                     |
//! | `tsb`               | TSBs / GSBs / SSMs for the VIN, and articles                          |
//! | `oasis`             | OASIS: vehicle, warranty, FSAs, warnings                              |
//! | `connected`         | Connected Vehicle: status, DTC history and descriptions, OTA, subscriptions |
//! | `labor`             | SLTS labor times                                                      |
//! | `diagnostics`       | the Diagnostics search                                                |
//!
//! The dealer code comes from the app's settings (set on first setup), never from the code.

use workbench_net::{Http, NetError, Url};

/// Where Ford PTS is
pub mod paths {
    pub const PTS: &str = "https://www.fordtechservice.dealerconnection.com";
    pub const PTS_HOST: &str = "www.fordtechservice.dealerconnection.com";
}

/// Signing in: the sign-in window starts at PTS (Ford sends it on to its sign-in pages when needed) and is done once
/// it's back on PTS. The cookies of these sites are taken.
pub mod signin {
    pub const START: &str = "https://www.fordtechservice.dealerconnection.com/";
    pub const DONE_HOST: &str = super::paths::PTS_HOST;
    pub const COOKIE_DOMAINS: &[&str] = &["dealerconnection.com", "ford.com"];
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The Ford sign-in has run out (Ford sent the request to its sign-in pages). The app signs in again.
    #[error("Sign in to PTS")]
    NeedsLogin,
    #[error("Ford returned HTTP {0}")]
    Status(u16),
    #[error(transparent)]
    Net(#[from] NetError),
    #[error("{0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One profile's Ford PTS: its requests, with that profile's cookies
#[derive(Clone)]
pub struct Ford {
    http: Http,
    base: Url,
    /// The dealership's P&A code, from settings
    dealer_code: String,
}

impl Ford {
    pub fn new(http: Http, dealer_code: impl Into<String>) -> Result<Self> {
        let base = Url::parse(paths::PTS).map_err(|_| Error::Format("PTS address".into()))?;
        Ok(Self {
            http,
            base,
            dealer_code: dealer_code.into(),
        })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn dealer_code(&self) -> &str {
        &self.dealer_code
    }

    pub fn http(&self) -> &Http {
        &self.http
    }
}
