use gofile::*;
use std::env::*;
use chrono::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();

    let api = Api::new().authorize(token);
    let account_details = api.get_account_details().await?;

    let dir = api.create_folder(account_details.root_folder, "001").await?;

    let content_id = dir.id;
    api.set_public_option(content_id, true).await?;
    api.set_password_option(content_id, "password").await?;
    api.set_description_option(content_id, "Dir Description").await?;
    api.set_expire_option(content_id, Utc::now() + Duration::days(1)).await?;
    api.set_tags_option(content_id, vec!["tag1", "tag2"]).await?;

    let server = api.get_server().await?;
    let upload_result = server.upload_file_with_filename_to_folder(dir.id, "test.txt", "file content").await?;
    let content_id = upload_result.file_id;
    let link_url = api.get_direct_link(content_id).await?;

    println!("{:?}", link_url);

    api.disable_direct_link(content_id).await?;

    Ok(())
}

