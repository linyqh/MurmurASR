//! Network test for the model downloader: cancel mid-way, resume, verify SHA-256.
//! Opt-in because it downloads ~254 MB: MURMUR_TEST_DOWNLOAD=1 cargo test --test download

use std::sync::{Arc, Mutex};

use murmur_lib::models::{self, Catalog, DownloadEvent, DownloadSource, Downloads};

#[tokio::test(flavor = "multi_thread")]
async fn download_resume_and_verify() {
    if std::env::var("MURMUR_TEST_DOWNLOAD").is_err() {
        eprintln!("skipping: set MURMUR_TEST_DOWNLOAD=1");
        return;
    }
    let source = match std::env::var("MURMUR_TEST_SOURCE").as_deref() {
        Ok("hfmirror") => DownloadSource::Hfmirror,
        Ok("modelscope") => DownloadSource::Modelscope,
        _ => DownloadSource::Auto,
    };
    let catalog = Catalog::builtin();
    let model = catalog.get("sensevoice-small-q8").unwrap().clone();
    let root = std::env::temp_dir().join(format!("murmur-dl-{}", std::process::id()));
    let downloads = Arc::new(Downloads::default());

    // First attempt: cancel once ~25% is on disk.
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let canceller = downloads.clone();
    let id = model.id.clone();
    models::download(
        downloads.clone(),
        root.clone(),
        model.clone(),
        source,
        move |e| {
            if let DownloadEvent::Progress {
                downloaded, total, ..
            } = &e
            {
                if *downloaded * 4 > *total {
                    canceller.cancel(&id);
                }
            }
            sink.lock().unwrap().push(e);
        },
    )
    .await;
    let last = events.lock().unwrap().last().cloned();
    assert!(
        matches!(last, Some(DownloadEvent::Cancelled { .. })),
        "expected cancel, got {last:?}"
    );
    let file = &model.files[0];
    let partial = models::model_dir(&root, &model).join(format!("{}.part", file.name));
    let kept = std::fs::metadata(&partial).unwrap().len();
    assert!(kept > 0 && kept < file.size, "partial size {kept}");
    eprintln!("cancelled with {kept} bytes kept");

    // Second attempt resumes from the partial file.
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    models::download(
        downloads.clone(),
        root.clone(),
        model.clone(),
        source,
        move |e| sink.lock().unwrap().push(e),
    )
    .await;
    let events = events.lock().unwrap();
    let first_progress = events.iter().find_map(|e| match e {
        DownloadEvent::Progress { downloaded, .. } => Some(*downloaded),
        _ => None,
    });
    assert!(
        first_progress.unwrap_or(0) >= kept,
        "did not resume: {first_progress:?} < {kept}"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, DownloadEvent::Verifying { .. })));
    assert!(
        matches!(events.last(), Some(DownloadEvent::Done { .. })),
        "last: {:?}",
        events.last()
    );
    let installed = models::installed(&root, &catalog);
    assert_eq!(installed.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
