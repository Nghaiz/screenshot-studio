//! Phase 4.1: ZIP and PDF export. A ZIP lists every exported file (same bytes, repeated names made unique);
//! a PDF has ceil(n / perPage) pages for every page size; bad requests write nothing; the background job
//! ends with a `finished` export event. Fixture libraries only (D10).

mod common {
    pub mod env;
    pub mod fixtures;
}

use std::collections::BTreeSet;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::env::{Env, blake3_of};
use ss_core::contract::ipc::*;
use ss_core::contract::{Caller, ErrorKind};
use ss_core::export::pdf_page_count;
use ss_core::jobs::{EventSink, NullSink};

fn ids(env: &Env) -> Vec<i64> {
    env.all().iter().map(|s| s.id).collect()
}

#[test]
fn zip_lists_every_file() {
    let env = Env::new(12);
    // Same name in another folder: the ZIP entry gets a ` (2)` suffix.
    let first = &env.fixtures[2];
    let dup_dir = env.root_dir().join("other");
    std::fs::create_dir_all(&dup_dir).unwrap();
    std::fs::copy(&first.path, dup_dir.join(&first.file_name)).unwrap();
    env.index(false);
    let rows = env.all();
    assert_eq!(rows.len(), 13);
    let dest = env.lib.path().join("out.zip");
    let report = env
        .core
        .export_zip_blocking(Caller::Ui, ExportZipReq { image_ids: ids(&env), dest_path: dest.to_string_lossy().into_owned() }, &NullSink)
        .expect("export zip");
    assert_eq!(report.files, 13);
    assert_eq!(report.bytes, std::fs::metadata(&dest).unwrap().len() as i64);

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).expect("readable zip");
    assert_eq!(zip.len(), 13, "one entry per file");
    let names: BTreeSet<String> = zip.file_names().map(|n| n.expect("entry name").into_owned()).collect();
    let (stem, ext) = first.file_name.rsplit_once('.').unwrap();
    assert!(names.contains(&first.file_name) && names.contains(&format!("{stem} (2).{ext}")), "{names:?}");
    for f in &env.fixtures {
        assert!(names.contains(&f.file_name), "{} missing from the zip", f.file_name);
    }
    // Every entry holds the original bytes, stored.
    let mut hashes_in_zip = BTreeSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        hashes_in_zip.insert(blake3::hash(&bytes).to_hex().to_string());
    }
    let on_disk: BTreeSet<String> = rows.iter().map(|s| blake3_of(std::path::Path::new(&env.detail(s.id).abs_path))).collect();
    assert_eq!(hashes_in_zip, on_disk);
}

#[test]
fn pdf_page_count_matches_per_page() {
    let env = Env::new(7);
    env.index(false);
    let all = ids(&env);
    for page_size in [PdfPageSize::A4, PdfPageSize::Fit] {
        for per_page in [1i64, 2, 4] {
            let dest = env.lib.path().join(format!("out-{page_size:?}-{per_page}.pdf"));
            let req = ExportPdfReq { image_ids: all.clone(), dest_path: dest.to_string_lossy().into_owned(), page_size, per_page };
            let report = env.core.export_pdf_blocking(Caller::Ui, req, &NullSink).expect("export pdf");
            let want = pdf_page_count(7, per_page as usize);
            assert_eq!(report.pages as usize, want, "{page_size:?} x {per_page}");
            let bytes = std::fs::read(&dest).unwrap();
            assert!(bytes.starts_with(b"%PDF"), "a PDF file");
            let mut warnings = Vec::new();
            let doc = printpdf::PdfDocument::parse(&bytes, &printpdf::PdfParseOptions { fail_on_error: false }, &mut warnings)
                .expect("the PDF parses back");
            assert_eq!(doc.pages.len(), want, "{page_size:?} x {per_page}: pages in the file");
        }
    }
    assert_eq!([pdf_page_count(7, 1), pdf_page_count(7, 2), pdf_page_count(7, 4), pdf_page_count(8, 4)], [7, 4, 2, 2]);
}

#[test]
fn bad_export_requests_write_nothing() {
    let env = Env::new(3);
    env.index(false);
    let all = ids(&env);
    let dest = env.lib.path().join("bad.pdf");
    let d = dest.to_string_lossy().into_owned();
    let pdf = |per_page, ids: Vec<i64>, dest: String| ExportPdfReq { image_ids: ids, dest_path: dest, page_size: PdfPageSize::A4, per_page };
    let e = env.core.export_pdf_blocking(Caller::Ui, pdf(3, all.clone(), d.clone()), &NullSink).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    let e = env.core.export_pdf_blocking(Caller::Ui, pdf(1, vec![], d.clone()), &NullSink).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidArgument);
    let e = env.core.export_pdf_blocking(Caller::Ui, pdf(1, vec![999_999], d.clone()), &NullSink).unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    let missing_dir = env.lib.path().join("nope").join("x.zip").to_string_lossy().into_owned();
    let e = env.core.export_zip_blocking(Caller::Ui, ExportZipReq { image_ids: all.clone(), dest_path: missing_dir }, &NullSink).unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotFound);
    // A file the export cannot read fails the job (no silent skip) and leaves no partial output.
    std::fs::remove_file(&env.fixtures[0].path).unwrap();
    let zip_dest = env.lib.path().join("partial.zip");
    let e = env
        .core
        .export_zip_blocking(Caller::Ui, ExportZipReq { image_ids: all, dest_path: zip_dest.to_string_lossy().into_owned() }, &NullSink)
        .unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Io | ErrorKind::NotFound), "{e:?}");
    assert!(!dest.exists() && !zip_dest.exists());
    let leftovers: Vec<_> = std::fs::read_dir(env.lib.path()).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().ends_with(".ss-tmp")).collect();
    assert!(leftovers.is_empty(), "no temp file left behind");
}

#[derive(Default)]
struct Rec(Mutex<Vec<JobProgress>>);

impl EventSink for Rec {
    fn job_progress(&self, p: &JobProgress) {
        self.0.lock().unwrap().push(p.clone());
    }
    fn library_changed(&self, _: &LibraryChanged) {}
}

#[test]
fn export_job_reports_progress_and_finishes() {
    let env = Env::new(5);
    env.index(false);
    let sink = Arc::new(Rec::default());
    let dest = env.lib.path().join("job.zip");
    let started = env
        .core
        .start_export_zip(Caller::Ui, ExportZipReq { image_ids: ids(&env), dest_path: dest.to_string_lossy().into_owned() }, sink.clone())
        .expect("job starts");
    let t0 = Instant::now();
    let last = loop {
        if let Some(p) = sink.0.lock().unwrap().iter().find(|p| p.finished).cloned() {
            break p;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "export job did not finish");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!((last.job_id, last.kind, last.error.clone()), (started.job_id, JobKind::Export, None));
    assert_eq!((last.done, last.total), (5, 5));
    assert!(dest.is_file());
}
