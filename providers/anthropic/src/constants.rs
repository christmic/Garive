pub(crate) const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
pub(crate) const API_KEY: &str = "x-api-key";
pub(crate) const AUTHORIZATION: &str = "authorization";
pub(crate) const VERSION_HEADER: &str = "anthropic-version";
pub(crate) const PROTOCOL_VERSION: &str = "2023-06-01";
pub(crate) const CONTENT_TYPE: &str = "content-type";
pub(crate) const ACCEPT: &str = "accept";
pub(crate) const RESERVED_HEADERS: &[&str] =
    &[API_KEY, AUTHORIZATION, VERSION_HEADER, CONTENT_TYPE, ACCEPT];
