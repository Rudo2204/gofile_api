#![allow(unused)]
pub mod bar;
mod payload;

use bar::WrappedBar;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use reqwest::{
    multipart::{Form, Part},
    Method, Response, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::cmp::min;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::sync::mpsc::UnboundedSender;
use url::Url;
use uuid::Uuid;

pub use payload::*;

pub struct UploadedMessage {
    pub uuid: Uuid,
    pub uploaded: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HttpRequestError: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("HttpStatusCodeError: url {0}, error code {1}")]
    HttpStatusCodeError(Url, StatusCode),

    #[error("ApiStatusError: url {0}, error {1}")]
    ApiStatusError(Url, String),

    #[error("Gofile returned empty server list")]
    EmptyServerList,

    #[error("InvalidFilePath at path {0}. Error: {1}")]
    InvalidFilePath(PathBuf, String),

    #[error("Could not open file at path {0}. Error: {1}")]
    CouldntOpenFile(PathBuf, String),

    #[error("Gofile InvalidContentUrl at url {0}. Error: {1}")]
    InvalidContentUrl(Url, String),

    #[error("StdIoError: {0}")]
    StdIoError(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct Api {
    pub base_url: String,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            base_url: "https://api.gofile.io".into(),
        }
    }
}

impl Api {
    pub async fn get_server(&self, uuid: Uuid) -> Result<ServerApi, Error> {
        let Servers { servers } = Api::get(&self.base_url, "servers").await?;
        let server = servers
            .into_iter()
            .filter(|x| x.zone == "eu")
            .next()
            .ok_or(Error::EmptyServerList)?
            .name;
        Ok(ServerApi {
            base_url: format!("https://{}.gofile.io", server),
            uuid,
        })
    }

    fn code_from_content_url(url: &Url) -> Result<String, Error> {
        let Some(mut segs) = url.path_segments() else {
            return Err(Error::InvalidContentUrl(
                url.clone(),
                "The content url must have path segments like '/d/XXXX'.".into(),
            ));
        };
        match segs.next() {
            Some("d") => (),
            _ => {
                return Err(Error::InvalidContentUrl(
                    url.clone(),
                    "The first path segment of content url must be 'd'.".into(),
                ))
            }
        };
        let Some(code) = segs.next() else {
            return Err(Error::InvalidContentUrl(
                url.clone(),
                "The content url must have two path segments like '/d/XXXX'.".into(),
            ));
        };
        Ok(code.into())
    }

    fn url(base_url: impl AsRef<str>, path: impl AsRef<str>) -> Url {
        let path = path.as_ref();
        Url::parse(&(format!("{}/{}", base_url.as_ref(), path))).unwrap()
    }

    async fn get<T>(base_url: impl AsRef<str>, path: impl AsRef<str>) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        Self::get_with_params(base_url, path, vec![]).await
    }

    async fn get_with_params<T>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        params: Vec<(&'static str, String)>,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut url = Self::url(base_url, path);
        for (key, value) in params {
            url.query_pairs_mut().append_pair(key, &value);
        }

        let res = reqwest::get(url).await?;
        Self::parse_res(res).await
    }

    async fn put_with_payload<T, P>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        Self::request_with_payload(Method::PUT, base_url, path, payload).await
    }

    async fn delete_with_payload<T, P>(
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        Self::request_with_payload(Method::DELETE, base_url, path, payload).await
    }

    async fn request_with_payload<T, P>(
        method: Method,
        base_url: impl AsRef<str>,
        path: impl AsRef<str>,
        payload: P,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        P: Serialize,
    {
        let url = Self::url(base_url, path);
        let client = reqwest::Client::new();
        let res = client.request(method, url).json(&payload).send().await?;
        Self::parse_res(res).await
    }

    async fn parse_res<T>(res: Response) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let status = res.status();
        let url = res.url().clone();
        if status != StatusCode::OK {
            return match res.json::<ApiResult<Value>>().await {
                Ok(res_obj) => Err(Error::ApiStatusError(url, res_obj.status)),
                Err(_) => Err(Error::HttpStatusCodeError(url, status)),
            };
        };

        let res_obj = res.json::<ApiResult<T>>().await?;
        if res_obj.status != "ok" {
            return Err(Error::ApiStatusError(url, res_obj.status));
        };

        Ok(res_obj.data)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerApi {
    pub base_url: String,
    pub uuid: Uuid,
}

impl ServerApi {
    pub async fn upload_file(
        &self,
        path: impl AsRef<Path>,
        tx: UnboundedSender<UploadedMessage>,
    ) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename(filename, file, tx).await
    }

    pub async fn upload_file_to_folder(
        &self,
        folder_id: Uuid,
        path: impl AsRef<Path>,
        tx: UnboundedSender<UploadedMessage>,
    ) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename, file, tx)
            .await
    }

    pub async fn upload_file_with_filename(
        &self,
        filename: impl Into<String>,
        body: File,
        tx: UnboundedSender<UploadedMessage>,
    ) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.base_url, filename, body, None, None, self.uuid, tx).await
    }

    pub async fn upload_file_with_filename_to_folder(
        &self,
        folder_id: Uuid,
        filename: impl Into<String>,
        body: File,
        tx: UnboundedSender<UploadedMessage>,
    ) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(
            &self.base_url,
            filename,
            body,
            Some(folder_id),
            None,
            self.uuid,
            tx,
        )
        .await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<(String, File), Error> {
        let path = path.as_ref();
        let Some(filename) = path.file_name() else {
            return Err(Error::InvalidFilePath(
                path.into(),
                "Couldn't get the filename.".into(),
            ));
        };
        let Some(filename) = filename.to_str() else {
            return Err(Error::InvalidFilePath(
                path.into(),
                "The filename couldn't convert to a utf-8 string.".into(),
            ));
        };

        let file = match File::open(path).await {
            Ok(file) => file,
            Err(err) => return Err(Error::CouldntOpenFile(path.into(), format!("{}", err))),
        };

        Ok((filename.into(), file))
    }

    async fn upload_file_impl(
        base_url: &str,
        filename: impl Into<String>,
        body: File,
        folder_id: Option<Uuid>,
        token: Option<String>,
        uuid: Uuid,
        tx: UnboundedSender<UploadedMessage>,
    ) -> Result<UploadedFile, Error> {
        let client = reqwest::Client::new();
        let file_name: String = filename.into();
        let input_ = file_name.clone();
        let output_ = String::from(base_url);

        let total_size = body.metadata().await?.len();
        let mut reader_stream = tokio_util::io::ReaderStream::new(body);
        let mut uploaded: u64 = 0;

        let async_stream = async_stream::stream! {
            while let Some(chunk) = reader_stream.next().await {
                if let Ok(chunk) = &chunk {
                    let new = min(uploaded + (chunk.len() as u64), total_size);
                    uploaded = new;
                    tx.send(UploadedMessage{ uuid, uploaded });
                    if uploaded >= total_size {
                        tx.send(UploadedMessage { uuid, uploaded: total_size });
                    }
                }
                yield chunk;
            }
        };

        let part = Part::stream(reqwest::Body::wrap_stream(async_stream)).file_name(file_name);
        let form = Form::new().part("file", part);

        let form = if let Some(folder_id) = folder_id {
            form.text("folderId", folder_id.to_string())
        } else {
            form
        };

        let form = if let Some(token) = token {
            form.text("token", token)
        } else {
            form
        };

        let url = Url::parse(&(format!("{}/contents/uploadfile", base_url))).unwrap();

        let res = client.post(url).multipart(form).send().await?;

        Api::parse_res(res).await
    }
}
