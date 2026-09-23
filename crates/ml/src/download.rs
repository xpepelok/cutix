use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::MlError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .build()
}

pub(crate) fn download_to(
    url: &str,
    target: &Path,
    on_progress: &mut dyn FnMut(f32),
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), MlError> {
    let directory = target.parent().ok_or_else(|| {
        MlError::Download(format!("{} has no parent directory", target.display()))
    })?;
    fs::create_dir_all(directory)
        .map_err(|error| MlError::Download(format!("create {}: {error}", directory.display())))?;

    let response = agent()
        .get(url)
        .call()
        .map_err(|error| MlError::Download(format!("GET {url}: {error}")))?;
    reject_html(response.header("Content-Type"), url)?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());

    let partial = partial_path(target);
    let result = write_body(
        response.into_reader(),
        &partial,
        total,
        on_progress,
        should_cancel,
    )
    .and_then(|()| {
        fs::rename(&partial, target)
            .map_err(|error| MlError::Download(format!("rename {}: {error}", partial.display())))
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    on_progress(1.0);
    Ok(())
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

fn reject_html(content_type: Option<&str>, url: &str) -> Result<(), MlError> {
    let is_html = content_type.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/html"))
    });
    if is_html {
        return Err(MlError::Download(format!(
            "GET {url}: server answered with an HTML page instead of the file"
        )));
    }
    Ok(())
}

fn write_body(
    mut reader: impl Read,
    partial: &Path,
    total: Option<u64>,
    on_progress: &mut dyn FnMut(f32),
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), MlError> {
    let mut file = fs::File::create(partial)
        .map_err(|error| MlError::Download(format!("create {}: {error}", partial.display())))?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = 0u64;

    loop {
        if should_cancel() {
            return Err(MlError::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| MlError::Download(format!("read: {error}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| MlError::Download(format!("write {}: {error}", partial.display())))?;
        written += read as u64;
        if let Some(total) = total.filter(|total| *total > 0) {
            on_progress((written as f32 / total as f32).clamp(0.0, 1.0));
        }
    }

    if let Some(total) = total
        && written != total
    {
        return Err(MlError::Download(format!(
            "connection closed after {written} of {total} bytes"
        )));
    }
    file.flush()
        .map_err(|error| MlError::Download(format!("write {}: {error}", partial.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("cutix-ml-download-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn html_responses_are_rejected() {
        assert!(reject_html(Some("text/html; charset=utf-8"), "u").is_err());
        assert!(reject_html(Some("TEXT/HTML"), "u").is_err());
        assert!(reject_html(Some("application/octet-stream"), "u").is_ok());
        assert!(reject_html(None, "u").is_ok());
    }

    #[test]
    fn a_short_body_is_reported_as_truncated() {
        let directory = scratch("short");
        let partial = directory.join("model.onnx.part");
        let error =
            write_body(&[1u8; 10][..], &partial, Some(20), &mut |_| {}, &|| false).unwrap_err();
        assert!(error.to_string().contains("10 of 20"));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_complete_body_is_written_with_progress() {
        let directory = scratch("complete");
        let partial = directory.join("model.onnx.part");
        let mut last = 0.0;
        write_body(
            &[1u8; 20][..],
            &partial,
            Some(20),
            &mut |done| last = done,
            &|| false,
        )
        .unwrap();
        assert_eq!(fs::read(&partial).unwrap().len(), 20);
        assert_eq!(last, 1.0);
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn cancellation_stops_the_body() {
        let directory = scratch("cancel");
        let partial = directory.join("model.onnx.part");
        let result = write_body(&[1u8; 20][..], &partial, None, &mut |_| {}, &|| true);
        assert!(matches!(result, Err(MlError::Cancelled)));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn partial_files_sit_next_to_the_target() {
        assert_eq!(
            partial_path(Path::new("cache/encoder_model.onnx")),
            PathBuf::from("cache/encoder_model.onnx.part")
        );
    }
}
