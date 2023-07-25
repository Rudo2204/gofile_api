use gofile::*;
use std::env::*;
use futures::future::*;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();
    let api = Api::authorize(token);
    let account_details = api.get_account_details().await?;

    let src_dir = api.create_folder(account_details.root_folder, "src").await?;
    let dst_dir = api.create_folder(account_details.root_folder, "dst").await?;

    let results = {
        let upload_task_1 = tokio::spawn(upload(api.clone(), src_dir.id, "test-001.txt", "file content 001"));
        let upload_task_2 = tokio::spawn(upload(api.clone(), src_dir.id, "test-002.txt", "file content 002"));

        join_all(vec![upload_task_1, upload_task_2]).await
    };

    let mut content_ids = Vec::new();
    for result in results {
        let upload_result = result.unwrap()?;
        content_ids.push(upload_result.file_id);
    }

    api.copy_content(content_ids, dst_dir.id).await?;

    Ok(())
}

async fn upload(api: AuthorizedApi, folder_id: Uuid, filename: &'static str, content: &'static str) -> Result<UploadedFile, Error> {
    let server = api.get_server().await?;
    server.upload_file_with_filename_to_folder(folder_id, filename.into(), content).await
}

