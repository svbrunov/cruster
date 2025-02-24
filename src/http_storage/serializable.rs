use base64;
use regex::Regex;
use serde_json as json;
use serde::{Serialize, Deserialize};
use http::{HeaderMap, header::HeaderName, HeaderValue as HTTPHeaderValue};

use std::{
    io::{Write, BufReader, BufRead},
    sync::mpsc::Receiver,
    str::FromStr,
    fs
};

use super::{RequestResponsePair, HTTPStorage};
use crate::{
    cruster_proxy::request_response::{
        HyperRequestWrapper,
        HyperResponseWrapper
    },
    utils::CrusterError,
    scope
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Header {
    key: String,
    encoding: String,
    value: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SerializableHTTPRequest {
    method: String,
    scheme: String,
    host: String,
    path: String,
    query: Option<String>,
    version: String,
    headers: Vec<Header>,
    body: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SerializableHTTPResponse {
    status: String,
    version: String,
    headers: Vec<Header>,
    body: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(super) struct SerializableProxyData {
    index: usize,
    request: SerializableHTTPRequest,
    response: Option<SerializableHTTPResponse>
}

impl SerializableHTTPRequest {
    fn get_uri(&self) -> String {
        match &self.query {
            Some(query) => {
                format!("{}{}{}{}", &self.scheme, &self.host, &self.path, query)
            },
            None => {
                format!("{}{}{}", &self.scheme, &self.host, &self.path)
            }
        }
    }
}

impl From<&HyperRequestWrapper> for SerializableHTTPRequest {
    fn from(request: &HyperRequestWrapper) -> Self {
        let host = request.get_hostname();
        let (path, query) = if let Ok(pth) = request.get_request_path_without_query() {
            let qr = request.get_query();
            (pth, qr)
        }
        else {
            (request.get_request_path(), None)
        };

        let headers: Vec<Header> = request.headers
            .iter()
            .map(|(k, v)| {
                let key = k.to_string();
                let (encoding, value) = if let Ok(decoded_value) = v.to_str() {
                    ("utf-8".to_string(), decoded_value.to_string())
                }
                else {
                    // HeaderValue::new("base64", base64::encode(v.as_bytes()))
                    ("base64".to_string(), base64::encode(v.as_bytes()))
                };

                Header {
                    key,
                    encoding,
                    value
                }
            })
            .collect();
        
        let body = if request.body.is_empty() {
            None
        }
        else {
            Some(base64::encode(request.body.as_slice()))
        };

        SerializableHTTPRequest {
            method: request.method.clone(),
            scheme: request.get_scheme(),
            host,
            path,
            query,
            version: request.version.clone(),
            headers,
            body
        }
    }
}

impl TryInto<HyperRequestWrapper> for SerializableHTTPRequest {
    type Error = CrusterError;
    fn try_into(self) -> Result<HyperRequestWrapper, Self::Error> {
        let uri = self.get_uri();
        let mut headers: HeaderMap<HTTPHeaderValue> = HeaderMap::default();
        for header in &self.headers {
            // TODO: we can improve it replacing clone with iterating over header parts
            let k = &header.key;
            let name = match HeaderName::from_str(k) {
                Ok(hname) => {
                    hname
                },
                Err(e) => {
                    return Err(CrusterError::UndefinedError(
                        format!("Could not parse HTTP Response header '{}' from file: {}", k, e)
                    ));
                }
            };

            let value_bytes: Vec<u8> = match header.encoding.as_ref() {
                "utf-8" => {
                    header.value.as_bytes().into()
                },
                "base64" => {
                    match base64::decode(header.value.as_str()) {
                        Ok(decoded) => {
                            decoded
                        },
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                },
                _ => {
                    return Err(CrusterError::UndefinedError(
                        format!("Could not parse response from file because of unknown header value encoding: {}", &header.encoding)
                    ));
                }
            };

            let value = HTTPHeaderValue::from_bytes(value_bytes.as_slice()).unwrap();
            headers.append(name.clone(), value);
        }

        let body = if let Some(body_encoded) = &self.body {
            match base64::decode(body_encoded) {
                Ok(body_bytes) => {
                    body_bytes
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        else {
            Vec::default()
        };

        Ok(
            HyperRequestWrapper {
                uri,
                method: self.method.to_string(),
                version: self.version.to_string(),
                headers,
                body
            }
        )
    }    
}

impl From<&HyperResponseWrapper> for SerializableHTTPResponse {
    fn from(response: &HyperResponseWrapper) -> Self {
        let headers: Vec<Header> = response.headers
            .iter()
            .map(|(k, v)| {
                let key = k.to_string();
                let (encoding, value) = if let Ok(decoded_value) = v.to_str() {
                    ("utf-8".to_string(), decoded_value.to_string())
                }
                else {
                    ("base64".to_string(), base64::encode(v.as_bytes()))
                };

                Header {
                    key,
                    encoding,
                    value
                }
            })
            .collect();

            let body = if response.body.is_empty() {
                None
            }
            else {
                Some(base64::encode(response.body.as_slice()))
            };

            SerializableHTTPResponse {
                status: response.status.clone(),
                version: response.version.clone(),
                headers,
                body
            }
    }
}

impl TryInto<HyperResponseWrapper> for SerializableHTTPResponse {
    type Error = CrusterError;
    fn try_into(self) -> Result<HyperResponseWrapper, Self::Error> {
        let mut headers: HeaderMap<HTTPHeaderValue> = HeaderMap::default();
        for header in &self.headers {
            // TODO: we can improve it replacing clone with iterating over header parts
            let k = &header.key;
            let name = match HeaderName::from_str(k) {
                Ok(hname) => {
                    hname
                },
                Err(e) => {
                    return Err(CrusterError::UndefinedError(
                        format!("Could not parse HTTP Response header '{}' from file: {}", k, e)
                    ));
                }
            };

            let value_bytes: Vec<u8> = match header.encoding.as_ref() {
                "utf-8" => {
                    header.value.as_bytes().into()
                },
                "base64" => {
                    match base64::decode(header.value.as_str()) {
                        Ok(decoded) => {
                            decoded
                        },
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                },
                _ => {
                    return Err(CrusterError::UndefinedError(
                        format!("Could not parse response from file because of unknown header value encoding: {}", &header.encoding)
                    ));
                }
            };

            let value = HTTPHeaderValue::from_bytes(value_bytes.as_slice()).unwrap();
            headers.append(name.clone(), value);
        }

        let body = if let Some(body_encoded) = &self.body {
            match base64::decode(body_encoded) {
                Ok(body_bytes) => {
                    body_bytes
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        else {
            Vec::default()
        };

        Ok(
            HyperResponseWrapper {
                status: self.status.clone(),
                version: self.version.clone(),
                headers,
                body
            }
        )
    }
}

impl TryFrom<&RequestResponsePair> for SerializableProxyData {
    type Error = CrusterError;
    fn try_from(pair: &RequestResponsePair) -> Result<Self, Self::Error> {
        return if pair.request.is_none() {
            Err(CrusterError::EmptyRequest(format!("Could not store record with id {} because  of empty request.", pair.index)))
        }
        else {
            Ok(
                Self {
                    index: pair.index.clone(),
                    request: SerializableHTTPRequest::from(pair.request.as_ref().unwrap()),
                    response: if let Some(rsp) = &pair.response {
                        Some(SerializableHTTPResponse::from(rsp))
                    }
                    else {
                        None
                    }
                }
            )
        };
    }
}

impl HTTPStorage {
    // 'Sentinel' used in a case when this method called in separate thread, in one-threaded case it can be None
    // It's needed to interrupt thread after some time expired, because rust threads cannot interrupt themselves 
    // https://internals.rust-lang.org/t/thread-cancel-support/3056
    pub(crate) fn store(&self, path: &str, sentinel: Option<Receiver<usize>>) -> Result<(), CrusterError> {
        let mut fout = fs::OpenOptions::new().write(true).open(path)?;
        for pair in &self.storage {
            let serializable_record = SerializableProxyData::try_from(pair)?;
            let jsn = json::to_string(&serializable_record)?;
            let _bytes_written = fout.write(jsn.as_bytes())?;
            let _one_byte_written = fout.write("\n".as_bytes())?;

            if let Some(rx) = &sentinel {
                if let Ok(max_duration) = rx.try_recv() {
                    return Err(CrusterError::JobDurateTooLongError(
                        format!("Process of storing proxy data was interrupted, it was running longer that {} seconds.", max_duration)
                    ));
                }
            }
        }

        Ok(())
    }

    fn insert_serializable_into_storage(&mut self, record: SerializableProxyData) -> Result<(), CrusterError> {
        let id = record.index;
        let request: HyperRequestWrapper = record.request.try_into()?;
        let response: Option<HyperResponseWrapper> = match record.response {
            Some(ser_respone) => {
                let response: HyperResponseWrapper = ser_respone.try_into()?;
                Some(response)
            },
            None => {
                None
            }
        };
        
        let pair = RequestResponsePair {
            index: record.index,
            request: Some(request),
            response
        };

        self.insert_with_explicit_id(id, pair);

        Ok(())
    }

    pub(crate) fn load(&mut self, load_path: &str) -> Result<(), CrusterError> {
        match std::fs::File::open(load_path) {
            Ok(fin) => {
                let reader = BufReader::new(fin);
                for read_result in reader.lines() {
                    if let Ok(line) = read_result {
                        let record: SerializableProxyData = json::from_str(&line)?;
                        self.insert_serializable_into_storage(record)?;
                    }
                }
            },
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(())
    }

    pub(crate) fn load_with_strict_scope(&mut self, load_path: &str, include: Option<&Vec<Regex>>, exclude: Option<&Vec<Regex>>) -> Result<(), CrusterError> {
        match std::fs::File::open(load_path) {
            Ok(fin) => {
                let reader = BufReader::new(fin);
                for read_result in reader.lines() {
                    if let Ok(line) = read_result {
                        let record: SerializableProxyData = json::from_str(&line)?;
                        let string_uri = record.request.get_uri();
                        let uri = string_uri.as_str();

                        let fit = match (include, exclude) {
                            (None, None) => {
                                true
                            },
                            (Some(included), None) => {
                                scope::fit_included(uri, included.as_slice())
                            },
                            (None, Some(excluded)) => {
                                scope::fit_excluded(uri, &excluded)
                            },
                            (Some(inc), Some(exc)) => {
                                scope::fit(uri, &inc, &exc)
                            }
                        };

                        if fit {
                            self.insert_serializable_into_storage(record)?;
                        }
                    }
                }
            },
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(())
    }
}