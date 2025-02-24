use hudsucker;

#[cfg(feature = "openssl-ca")]
use hudsucker::{
    certificate_authority::{OpensslAuthority as HudSuckerCA},
    openssl::{hash::MessageDigest, pkey::PKey, x509::X509},
};

#[cfg(feature = "rcgen-ca")]
use hudsucker::{
    certificate_authority::{RcgenAuthority as HudSuckerCA},
};

#[cfg(feature = "rcgen-ca")]
use hudsucker::rustls::{PrivateKey, Certificate};

use serde_json;
use base64::DecodeError;
use rcgen::{CertificateParams, self, IsCa, BasicConstraints, Certificate as RCgenCertificate, PKCS_ECDSA_P256_SHA256};

use std::{
    io::{self, },
    fmt::{self, Debug},
    num::ParseIntError,
    net::AddrParseError,
    fs
};

use serde_yaml;
// use std::time::macros::datetime;
use time::OffsetDateTime;
use time::macros::datetime;
use tokio::sync::mpsc::error::{SendError as tokio_SendError, TryRecvError};
use crate::CrusterWrapper;
use regex::Error as regex_error;

use log::debug;

#[derive(Debug, Clone)]
pub(crate) enum CrusterError {
    IOError(String),
    // OpenSSLError(String),
    HudSuckerError(String),
    ConfigError(String),
    PortParsingError(String),
    ParseAddressError(String),
    // RenderUnitCastError(String),
    UndefinedError(String),
    // NotParagraphRenderUnit(String),
    SendError(String),
    HyperBodyParseError(String),
    HeaderToStringError(String),
    TryRecvError(String),
    // UnknownResponseBodyEncoding(String),
    // NotImplementedError(String),
    UnacceptableFilter(String),
    ProxyTableIndexOutOfRange(String),
    CouldParseRequestPathError(String),
    EmptyRequest(String),
    JSONError(String),
    JobDurateTooLongError(String),
    Base64DecodeError(String),
    StorePathNotFoundError(String),
}

impl From<io::Error> for CrusterError {
    fn from(e: io::Error) -> Self { Self::IOError(e.to_string()) }
}

// impl From<openssl::error::Error> for CrusterError {
//     fn from(e: openssl::error::Error) -> Self { Self::OpenSSLError(e.to_string()) }
// }

// impl From<openssl::error::ErrorStack> for CrusterError {
//     fn from(e: openssl::error::ErrorStack) -> Self { Self::OpenSSLError(e.to_string()) }
// }

impl From<DecodeError> for CrusterError {
    fn from(e: DecodeError) -> Self { Self::Base64DecodeError(e.to_string()) }
}

impl From<hudsucker::Error> for CrusterError {
    fn from(e: hudsucker::Error) -> Self { Self::HudSuckerError(e.to_string()) }
}

impl From<String> for CrusterError {
    fn from(s: String) -> Self { Self::UndefinedError(s.to_string()) }
}

impl From<ParseIntError> for CrusterError {
    fn from(e: ParseIntError) -> Self {
        Self::PortParsingError(
            format!("Unable to parse port number: {}", e)
        )
    }
}

impl From<http::header::ToStrError> for CrusterError {
    fn from(e: http::header::ToStrError) -> Self {
        Self::HeaderToStringError(
            format!("Unable to parse header value into string: {}", e.to_string())
        )
    }
}

impl From<AddrParseError> for CrusterError {
    fn from(e: AddrParseError) -> Self { Self::ParseAddressError(e.to_string()) }
}

impl From<serde_yaml::Error> for CrusterError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::ConfigError(
            format!("Unable to serialize/deserialize YAML data: {}", e.to_string())
        )
    }
}

impl From<serde_json::Error> for CrusterError {
    fn from(e: serde_json::Error) -> Self {
        Self::JSONError(
            format!("Unable to serialize/deserialize JSON data: {}", e.to_string())
        )
    }
}

impl From<tokio_SendError<(CrusterWrapper, usize)>> for CrusterError {
    fn from(e: tokio_SendError<(CrusterWrapper, usize)>) -> Self {
        Self::SendError(
            format!("Unable communicate with other thread: {}", e.to_string())
        )
    }
}

impl From<hyper::Error> for CrusterError {
    fn from(e: hyper::Error) -> Self {
        Self::HyperBodyParseError(
            format!("Unable to parse hyper body: {}", e.to_string())
        )
    }
}

impl From<TryRecvError> for CrusterError {
    fn from(e: TryRecvError) -> Self {
        Self::TryRecvError(
            format!("Could not receive http message from proxy: {}", e.to_string())
        )
    }
}

impl From<regex_error> for CrusterError {
    fn from(e: regex_error) -> Self {
        Self::UnacceptableFilter(
            format!("Could not set filter because of error: {}", e.to_string())
        )
    }
}

impl fmt::Display for CrusterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CrusterError::ConfigError(s) => {
                write!(f, "An error occurred while handling input parameters: {}\n{}",
                       s,
                       "Enter '-h' for help."
                )
            },
            CrusterError::UndefinedError(s) => {
                write!(f, "{}", s)
            },
            // CrusterError::NotImplementedError(s) => {
            //     write!(f, "{}", s)
            // },
            // CrusterError::UnknownResponseBodyEncoding(s) => {
            //     write!(f, "{}", s)
            // },
            CrusterError::UnacceptableFilter(s) => {
                write!(f, "{}", s)
            },
            CrusterError::ProxyTableIndexOutOfRange(s) => {
                write!(f, "{}", s)
            },
            CrusterError::EmptyRequest(s) => {
                write!(f, "{}", s)
            },
            CrusterError::JSONError(s) => {
                write!(f, "{}", s)
            },
            CrusterError::JobDurateTooLongError(s) => {
                write!(f, "{}", s)
            },
            CrusterError::Base64DecodeError(s) => {
                write!(f, "{}", s)
            },
            CrusterError::StorePathNotFoundError(s) => {
                write!(f, "{}", s)
            }
            _ => { write!(f, "{:?}", self) }
        }
    }
}

// ---------------------------------------------------------------------------------------------- //

pub(crate) fn get_ca(key_path: &str, cer_path: &str) -> Result<HudSuckerCA, CrusterError> {
    use std::io::Read;

    let mut key_buffer: Vec<u8> = Vec::new();
    let f = fs::File::open(key_path);
    match f {
        Ok(mut file) => {
            let res = file.read_to_end(&mut key_buffer);
            if let Err(e) = res {
                return Err(
                    CrusterError::IOError(
                        format!("Could not read from key file, info: {}", e.to_string())
                    )
                )
            }
        },
        Err(e) => return Err(
            CrusterError::IOError(
                format!("Could not find or open key file, info: {}", e.to_string())
            )
        )
    }

    let mut cer_buffer: Vec<u8> = Vec::new();
    let f = fs::File::open(cer_path);
    match f {
        Ok(mut file) => {
            let res = file.read_to_end(&mut cer_buffer);
            if let Err(e) = res {
                return Err(
                    CrusterError::IOError(
                        format!("Could not read from cer file, info: {}", e.to_string())
                    )
                )
            }
        },
        Err(e) => return Err(
            CrusterError::IOError(
                format!("Could not find or open cer file, info: {}", e.to_string())
            )
        )
    }

    #[cfg(feature = "openssl-ca")]
    return {
        debug!("openssl-ca feature enabled");

        let private_key = PKey::private_key_from_pem(&key_buffer).unwrap();
        let ca_cert = X509::from_pem(&cer_buffer).unwrap();

        Ok(HudSuckerCA::new(private_key, ca_cert, MessageDigest::sha256(), 1_000))
    };

    #[cfg(feature = "rcgen-ca")]
    return {
        debug!("rcgen-ca feature enabled");

        let mut key_buffer_ref = key_buffer.as_slice();
        let mut cert_buffer_ref = cer_buffer.as_slice();

        let mut private_key_raw = rustls_pemfile::pkcs8_private_keys(&mut key_buffer_ref).unwrap();
        let mut ca_cert_raw = rustls_pemfile::certs(&mut cert_buffer_ref).unwrap();

        let private_key = PrivateKey(private_key_raw.remove(0));
        let ca_cert = Certificate(ca_cert_raw.remove(0));

        Ok(HudSuckerCA::new(private_key, ca_cert, 1000).unwrap())
    };
}

pub(crate) fn generate_key_and_cer(key_path: &str, cer_path: &str) {
    if std::path::Path::new(key_path).exists() && std::path::Path::new(key_path).exists() {
        return;
    }

    let mut cert_params = CertificateParams::default();

    cert_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    cert_params.not_before = OffsetDateTime::from(datetime!(1970-01-01 0:00 UTC));
    cert_params.not_after = OffsetDateTime::from(datetime!(5000-01-01 0:00 UTC));
    cert_params.key_pair = None;
    cert_params.alg = &PKCS_ECDSA_P256_SHA256;

    let new_cert = RCgenCertificate::from_params(cert_params).unwrap();
    fs::write(cer_path, new_cert.serialize_pem().unwrap()).unwrap();
    fs::write(key_path, new_cert.serialize_private_key_pem()).unwrap();
}
