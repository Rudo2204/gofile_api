mod payload;

use std::{
    borrow::{
        Cow,
    },
    path::{
        Path,
        PathBuf,
    },
};

use url::{
    Url,
};

use reqwest::{
    Response,
    StatusCode,
    Body,
    Method,
    multipart::{
        Form,
        Part,
    },
};

use serde::{
    Serialize,
    Deserialize,
    de::{
        DeserializeOwned,
    },
};

use serde_json::{
    Value,
};

use tokio::{
    fs::{
        File,
    },
};

use uuid::{
    Uuid,
};

use chrono::{
    DateTime,
    Utc,
};

pub use payload::{
    CreateFolderApiPayload,
    SetOptionApiPayload,
    CopyContentApiPayload,
    DeleteContentApiPayload,
    ContentOpt,
    Server,
    ApiResult,
    UploadedFile,
    Content,
    AccountDetails,
    NoInfo,
};

#[derive(Debug)]
pub enum Error {
    HttpRequestError(Option<Url>, String),
    HttpStatusCodeError(Url, StatusCode),
    ApiStatusError(Url, String),
    InvalidFilePath(PathBuf, String),
    CouldntOpenFile(PathBuf, String),
    InvalidContentUrl(Url, String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::HttpRequestError(err.url().map(|u| u.clone()), err.to_string())
    }
}

#[derive(Debug)]
pub struct Api {
}

impl Api {
    pub fn authorize(token: impl Into<String>) -> AuthorizedApi {
        AuthorizedApi { token: token.into() }
    }

    pub async fn get_server() -> Result<ServerApi, Error> {
        let Server { server } = Self::get("getServer").await?;
        Ok(ServerApi { server })
    }

    fn code_from_content_url(url: &Url) -> Result<String, Error> {
        let Some(mut segs) = url.path_segments() else {
            return Err(Error::InvalidContentUrl(url.clone(), "The content url must have path segments like '/d/XXXX'.".into()));
        };
        match segs.next() {
            Some("d") => (),
            _ => return Err(Error::InvalidContentUrl(url.clone(), "The first path segment of content url must be 'd'.".into())),
        };
        let Some(code) = segs.next() else {
            return Err(Error::InvalidContentUrl(url.clone(), "The content url must have two path segments like '/d/XXXX'.".into()));
        };
        Ok(code.into())
    }

    fn url(path: impl AsRef<str>) -> Url {
        let path = path.as_ref();
        Url::parse(&(format!("https://api.gofile.io/{}", path))).unwrap()
    }

    async fn get<T>(path: impl AsRef<str>) -> Result<T, Error>
        where
            T: DeserializeOwned,
    {
        Self::get_with_params(path, vec![]).await
    }

    async fn get_with_params<T>(path: impl AsRef<str>, params: Vec<(&'static str, String)>) -> Result<T, Error>
        where
            T: DeserializeOwned,
    {
        let mut url = Self::url(path);
        for (key, value) in params {
            url.query_pairs_mut().append_pair(key, &value);
        };
        
        let res = reqwest::get(url).await?;
        Self::parse_res(res).await
    }

    async fn put_with_payload<T, P>(path: impl AsRef<str>, payload: P) -> Result<T, Error>
        where
            T: DeserializeOwned,
            P: Serialize,
    {
        Self::request_with_payload(Method::PUT, path, payload).await
    }

    async fn delete_with_payload<T, P>(path: impl AsRef<str>, payload: P) -> Result<T, Error>
        where
            T: DeserializeOwned,
            P: Serialize,
    {
        Self::request_with_payload(Method::DELETE, path, payload).await
    }

    async fn request_with_payload<T, P>(method: Method, path: impl AsRef<str>, payload: P) -> Result<T, Error>
        where
            T: DeserializeOwned,
            P: Serialize,
    {
        let url = Self::url(path);
        let client = reqwest::Client::new();
        let res = client
            .request(method, url)
            .json(&payload)
            .send()
            .await?;
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

#[derive(Clone, Debug)]
pub struct AuthorizedApi {
    pub token: String,
}

impl AuthorizedApi {
    pub async fn get_server(&self) -> Result<AuthorizedServerApi, Error> {
        let ServerApi { server } = Api::get_server().await?;
        Ok(AuthorizedServerApi { server, token: self.token.clone() })
    }

    pub async fn get_content(&self, url: &Url) -> Result<Content, Error> {
        let code = Api::code_from_content_url(url)?;
        self.get_content_by_code(code).await
    }

    pub async fn get_content_by_id(&self, content_id: Uuid) -> Result<Content, Error> {
        self.get_content_impl(content_id.to_string()).await
    }

    pub async fn get_content_by_code(&self, code: impl Into<String>) -> Result<Content, Error> {
        self.get_content_impl(code).await
    }

    async fn get_content_impl(&self, id_or_code: impl Into<String>) -> Result<Content, Error> {
        Api::get_with_params("getContent", vec![("contentId", id_or_code.into()), ("token", self.token.clone())]).await
    }

    pub async fn get_account_details(&self) -> Result<AccountDetails, Error> {
        Api::get_with_params("getAccountDetails", vec![("token", self.token.clone())]).await
    }

    pub async fn create_folder(&self, parent_folder_id: Uuid, folder_name: impl Into<String>) -> Result<Content, Error> {
        Api::put_with_payload("createFolder", CreateFolderApiPayload {
            token: self.token.clone(),
            parent_folder_id,
            folder_name: folder_name.into(),
        }).await
    }

    pub async fn set_public_option(&self, content_id: Uuid, public: bool) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Public(public)).await
    }

    pub async fn set_password_option(&self, content_id: Uuid, password: impl Into<String>) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Password(password.into())).await
    }

    pub async fn set_description_option(&self, content_id: Uuid, description: impl Into<String>) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Description(description.into())).await
    }

    pub async fn set_expire_option(&self, content_id: Uuid, expire: DateTime<Utc>) -> Result<NoInfo, Error> {
        self.set_option(content_id, ContentOpt::Expire(expire)).await
    }

    pub async fn set_tags_option<S>(&self, content_id: Uuid, tags: Vec<S>) -> Result<NoInfo, Error>
    where
        S: Into<String>,
    {
        self.set_option(content_id, ContentOpt::Tags(tags.into_iter().map(|s| s.into()).collect())).await
    }

    pub async fn get_direct_link(&self, content_id: Uuid) -> Result<Url, Error> {
        self.set_direct_link_option(content_id, true).await
    }

    pub async fn disable_direct_link(&self, content_id: Uuid) -> Result<NoInfo, Error> {
        self.set_direct_link_option(content_id, false).await
    }

    pub async fn set_direct_link_option<T>(&self, content_id: Uuid, direct_link: bool) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        self.set_option(content_id, ContentOpt::DirectLink(direct_link)).await
    }

    pub async fn set_option<T>(&self, content_id: Uuid, opt: ContentOpt) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        Api::put_with_payload("setOption", SetOptionApiPayload {
            token: self.token.clone(),
            content_id,
            opt,
        }).await
    }

    pub async fn copy_content(&self, content_ids: Vec<Uuid>, dest_folder_id: Uuid) -> Result<NoInfo, Error> {
        Api::put_with_payload("copyContent", CopyContentApiPayload {
            token: self.token.clone(),
            contents_id: content_ids,
            folder_id_dest: dest_folder_id,
        }).await
    }

    pub async fn delete_content(&self, content_ids: Vec<Uuid>) -> Result<NoInfo, Error> {
        Api::delete_with_payload("deleteContent", DeleteContentApiPayload {
            token: self.token.clone(),
            contents_id: content_ids,
        }).await
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerApi {
    pub server: String,
}

impl ServerApi {
    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename(filename.into(), file).await
    }

    pub async fn upload_file_to_folder(&self, folder_id: Uuid, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = Self::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename.into(), file).await
    }

    pub async fn upload_file_with_filename(&self, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.server, filename, body, None, None).await
    }

    pub async fn upload_file_with_filename_to_folder(&self, folder_id: Uuid, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        Self::upload_file_impl(&self.server, filename, body, Some(folder_id), None).await
    }

    pub async fn open_file(path: impl AsRef<Path>) -> Result<(String, File), Error> {
        let path = path.as_ref();
        let Some(filename) = path.file_name() else {
            return Err(Error::InvalidFilePath(path.into(), "Couldn't get the filename.".into()));
        };
        let Some(filename) = filename.to_str() else {
            return Err(Error::InvalidFilePath(path.into(), "The filename couldn't convert to a utf-8 stirng.".into()));
        };

        let file = match File::open(path).await {
            Ok(file) => file,
            Err(err) => return Err(Error::CouldntOpenFile(path.into(), format!("{}", err))),
        };

        Ok((filename.into(), file))
    }

    async fn upload_file_impl(server: &str, filename: Cow<'static, str>, body: impl Into<Body>, folder_id: Option<Uuid>, token: Option<String>) -> Result<UploadedFile, Error> {
        let client = reqwest::Client::new();

        let part = Part::stream(body)
            .file_name(filename);
        let form = Form::new()
            .part("file", part);

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

        let url = Url::parse(&(format!("https://{}.gofile.io/uploadFile", server))).unwrap();

        let res = client.post(url)
            .multipart(form)
            .send()
            .await?;

        Api::parse_res(res).await
    }
}

#[derive(Clone, Debug)]
pub struct AuthorizedServerApi {
    pub server: String,
    pub token: String,
}

impl AuthorizedServerApi {
    pub fn authorize(self, token: impl Into<String>) -> AuthorizedServerApi {
        AuthorizedServerApi { server: self.server, token: token.into() }
    }

    pub async fn upload_file(&self, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename(filename.into(), file).await
    }

    pub async fn upload_file_to_folder(&self, folder_id: Uuid, path: impl AsRef<Path>) -> Result<UploadedFile, Error> {
        let (filename, file) = ServerApi::open_file(path).await?;
        self.upload_file_with_filename_to_folder(folder_id, filename.into(), file).await
    }

    pub async fn upload_file_with_filename(&self, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(&self.server, filename, body, None, Some(self.token.clone())).await
    }

    pub async fn upload_file_with_filename_to_folder(&self, folder_id: Uuid, filename: Cow<'static, str>, body: impl Into<Body>) -> Result<UploadedFile, Error> {
        ServerApi::upload_file_impl(&self.server, filename, body, Some(folder_id), Some(self.token.clone())).await
    }
}

