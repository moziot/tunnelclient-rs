// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use acme_client::Directory;
use acme_client::error::Error as AcmeError;
use types::{Discovered, NameAndToken, ServerInfo};
use url::percent_encoding::{percent_encode, QUERY_ENCODE_SET};
use reqwest::{Client, Error as ReqwestError, StatusCode};
use std::convert::From;
use std::path::Path;

pub struct TunnelClient {
    pub tunnel_url: String,
    pub token: Option<String>,
}

#[derive(Debug)]
pub enum TunnelClientError {
    NoName,
    NoToken,
    NoChallenge,
    BadRequest,
    Other(String),
    Acme(AcmeError),
}

fn url_param(param: &str) -> String {
    percent_encode(param.as_bytes(), QUERY_ENCODE_SET).collect::<String>()
}

impl From<AcmeError> for TunnelClientError {
    fn from(err: AcmeError) -> Self {
        TunnelClientError::Acme(err)
    }
}

impl From<ReqwestError> for TunnelClientError {
    fn from(err: ReqwestError) -> Self {
        TunnelClientError::Other(format!("{}", err))
    }
}

// Macros that helps with declaring API endpoints.
macro_rules! api_endpoint {
    ($name:ident, $base:expr, $with_token:expr, $ret:ty) => (
        pub fn $name(&self, params: &[(&str, Option<&str>)]) -> Result<$ret, TunnelClientError> {
            if $with_token {
                if self.token.is_none() {
                    error!("No token available!");
                    return Err(TunnelClientError::NoToken);
                }
            }

            let client = Client::new().expect("Client creation failure");
            match client
                    .get(&self.get_full_url($base, params, $with_token))
                    .send() {
                Ok(mut response) => {
                    if *response.status() == StatusCode::Ok {
                        let res: Result<$ret, ReqwestError> = response.json();
                        match res {
                            Ok(res) => Ok(res),
                            Err(err) => Err(TunnelClientError::from(err)),
                        }
                    } else {
                        Err(TunnelClientError::BadRequest)
                    }
                }
                Err(err) => Err(TunnelClientError::from(err)),
            }
        }
    )
}

// Special case for empty answers.
macro_rules! empty_api_endpoint {
    ($name:ident, $base:expr, $with_token:expr) => (
        pub fn $name(&self, params: &[(&str, Option<&str>)]) -> Result<(), TunnelClientError> {
            if $with_token {
                if self.token.is_none() {
                    error!("No token available!");
                    return Err(TunnelClientError::NoToken);
                }
            }

            let client = Client::new().expect("Client creation failure");
            match client
                    .get(&self.get_full_url($base, params, $with_token))
                    .send() {
                Ok(response) => {
                    if *response.status() == StatusCode::Ok {
                        Ok(())
                    } else {
                        Err(TunnelClientError::BadRequest)
                    }
                }
                Err(err) => Err(TunnelClientError::from(err)),
            }
        }
    )
}

impl TunnelClient {
    pub fn new(tunnel_url: &str, token: Option<String>) -> Self {
        TunnelClient {
            tunnel_url: tunnel_url.to_owned(),
            token: token,
        }
    }

    fn get_full_url(&self,
                    path: &str,
                    params: &[(&str, Option<&str>)],
                    include_token: bool)
                    -> String {
        let mut url = format!("{}/{}", self.tunnel_url, path);
        let mut sep = "?";
        for param in params {
            if let Some(pvalue) = param.1 {
                url.push_str(&format!("{}{}={}", sep, param.0, url_param(pvalue)));
                sep = "&";
            }
        }

        if include_token {
            if let Some(ref token) = self.token {
                url.push_str(&format!("{}token={}", sep, url_param(token)));
            }
        }

        url
    }

    api_endpoint!(call_subscribe, "subscribe", false, NameAndToken);
    pub fn subscribe(&self, name: &str, description: Option<&str>) -> Option<Self> {
        match self.call_subscribe(&[("name", Some(name)), ("desc", description)]) {
            Ok(n_t) => {
                Some(TunnelClient {
                         tunnel_url: self.tunnel_url.clone(),
                         token: Some(n_t.token),
                     })
            }
            Err(_) => None,
        }
    }

    empty_api_endpoint!(call_unsubscribe, "unsubscribe", true);
    pub fn unsubscribe(&self) -> Result<(), TunnelClientError> {
        self.call_unsubscribe(&[])
    }

    empty_api_endpoint!(call_register, "register", true);
    pub fn register(&self, local_ip: &str) -> Result<(), TunnelClientError> {
        self.call_register(&[("local_ip", Some(local_ip))])
    }

    empty_api_endpoint!(call_dnsconfig, "dnsconfig", true);
    pub fn dnsconfig(&self, challenge: &str) -> Result<(), TunnelClientError> {
        self.call_dnsconfig(&[("challenge", Some(challenge))])
    }

    api_endpoint!(call_info, "info", true, ServerInfo);
    pub fn info(&self) -> Result<ServerInfo, TunnelClientError> {
        self.call_info(&[])
    }

    api_endpoint!(call_ping, "ping", true, Discovered);
    pub fn ping(&self) -> Result<Discovered, TunnelClientError> {
        self.call_ping(&[])
    }

    empty_api_endpoint!(call_adddiscovery, "adddiscovery", true);
    pub fn adddiscovery(&self, disco: &str) -> Result<(), TunnelClientError> {
        self.call_adddiscovery(&[("disco", Some(disco))])
    }

    empty_api_endpoint!(call_revokediscovery, "adddiscovery", true);
    pub fn revokediscovery(&self, disco: &str) -> Result<(), TunnelClientError> {
        self.call_revokediscovery(&[("disco", Some(disco))])
    }

    empty_api_endpoint!(call_setemail, "setemail", true);
    pub fn setemail(&self, email: &str) -> Result<(), TunnelClientError> {
        self.call_setemail(&[("email", Some(email))])
    }

    empty_api_endpoint!(call_revokeemail, "revokeemail", true);
    pub fn revokeemail(&self, email: &str) -> Result<(), TunnelClientError> {
        self.call_revokeemail(&[("email", Some(email))])
    }

    // Starts the LE flow.
    pub fn lets_encrypt(&self,
                        domain: &str,
                        name: &str,
                        path: &Path)
                        -> Result<(), TunnelClientError> {
        if self.token.is_none() {
            error!("No token available to retrieve the certificate for {}",
                   domain);
            return Err(TunnelClientError::NoToken);
        }

        let directory = Directory::lets_encrypt()?;
        let account = directory.account_registration().register()?;

        let remote_domain = format!("{}.box.{}", name, domain);
        let local_domain = format!("local.{}.box.{}", name, domain);

        let domains = [remote_domain.as_str(), local_domain.as_str()];

        for domain in &domains {
            let authorization = account.authorization(domain)?;
            let dns_challenge = match authorization.get_dns_challenge() {
                Some(challenge) => challenge,
                None => return Err(TunnelClientError::NoChallenge),
            };
            let signature = dns_challenge.signature()?;

            self.dnsconfig(&signature)?;

            dns_challenge.validate()?;
            info!("DNS challenge validated for {}", domain);
        }

        let certificate_signer = account.certificate_signer(&domains);
        let cert = certificate_signer.sign_certificate()?;
        cert.save_signed_certificate_and_chain(None, path.join("certificate.pem"))?;
        cert.save_private_key(path.join("privatekey.pem"))?;
        info!("Certificate and private key for {} saved.", domain);
        Ok(())
    }
}
