use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use slint::ComponentHandle;
use tempfile::TempDir;
use wsdist_app::{
    AppPaths, ResourceError, Resources, create_resource_error_window, load_resources_at,
};
use wsdist_core::CalcError;

mod resource_support;

const REQUIRED_FILES: [&str; 4] = [
    "data/enemies.json5",
    "data/buffs.json5",
    "data/pup-attachments.json5",
    "item_list.csv",
];

fn populate_resources(root: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::create_dir_all(root.join("data")).unwrap();
    for relative in REQUIRED_FILES {
        fs::copy(source.join(relative), root.join(relative)).unwrap();
    }
    resource_support::copy_gear(&source.join("data/gear"), &root.join("data/gear")).unwrap();
}

fn executable_at(root: &Path) -> PathBuf {
    root.join("wsdist-gui.exe")
}

fn assert_data_error(result: Result<Resources, ResourceError>, expected_path: &Path) {
    match result {
        Err(ResourceError::Catalog(CalcError::Data { path, .. })) => {
            assert_eq!(path, expected_path);
        }
        Err(error) => panic!("expected a catalog data error for {expected_path:?}, got {error}"),
        Ok(_) => panic!("unexpectedly loaded resources with invalid {expected_path:?}"),
    }
}

#[test]
fn portable_archive_loads_from_an_unrelated_working_directory() {
    let archive = TempDir::new().unwrap();
    let other_directory = TempDir::new().unwrap();
    populate_resources(archive.path());
    let paths = AppPaths::from_parts(
        None,
        &executable_at(archive.path()),
        other_directory.path(),
    )
    .unwrap();

    let loaded = load_resources_at(paths).unwrap();
    assert_eq!(loaded.paths.root, archive.path());
    assert_eq!(loaded.catalog.by_name2("Empty"), Some(loaded.catalog.empty));
    assert_eq!(loaded.catalog.piece(loaded.catalog.empty).name.as_ref(), "Empty");
}

#[test]
fn absolute_override_takes_precedence_without_using_other_paths() {
    let archive = TempDir::new().unwrap();
    let paths = AppPaths::from_parts(
        Some(archive.path().as_os_str()),
        Path::new("an-unused-relative-executable"),
        Path::new("an-unused-relative-working-directory"),
    )
    .unwrap();
    assert_eq!(paths.root, archive.path());
}

#[test]
fn relative_override_is_resolved_against_the_supplied_working_directory() {
    let launch_directory = TempDir::new().unwrap();
    let paths = AppPaths::from_parts(
        Some(OsStr::new("portable archive")),
        Path::new("an-unused-relative-executable"),
        launch_directory.path(),
    )
    .unwrap();
    let root = launch_directory.path().join("portable archive");
    assert_eq!(paths.root, root);
    assert_data_error(load_resources_at(paths), &root.join("data/gear"));
}

#[test]
fn invalid_selected_roots_never_fall_back_to_other_valid_roots() {
    let valid = TempDir::new().unwrap();
    let missing = TempDir::new().unwrap();
    populate_resources(valid.path());

    let executable_root = AppPaths::from_parts(
        None,
        &executable_at(missing.path()),
        valid.path(),
    )
    .unwrap();
    assert_data_error(
        load_resources_at(executable_root),
        &missing.path().join("data/gear"),
    );

    let override_root = AppPaths::from_parts(
        Some(missing.path().as_os_str()),
        &executable_at(valid.path()),
        valid.path(),
    )
    .unwrap();
    assert_data_error(
        load_resources_at(override_root),
        &missing.path().join("data/gear"),
    );
}

#[test]
fn legacy_monolith_never_replaces_the_required_gear_fragment_directory() {
    let archive = TempDir::new().unwrap();
    populate_resources(archive.path());
    let gear = archive.path().join("data/gear");
    let (fragment, _) = resource_support::piece_fragment(&gear, "Empty").unwrap();
    fs::copy(fragment, archive.path().join("data/gear.json5")).unwrap();
    fs::remove_dir_all(&gear).unwrap();
    let paths = AppPaths::from_parts(
        None,
        &executable_at(archive.path()),
        Path::new("unused"),
    )
    .unwrap();
    assert_data_error(load_resources_at(paths), &gear);
}

#[test]
fn empty_override_is_an_error_instead_of_an_implicit_working_directory_root() {
    let root = TempDir::new().unwrap();
    let error = AppPaths::from_parts(
        Some(OsStr::new("")),
        &executable_at(root.path()),
        root.path(),
    )
    .unwrap_err();
    assert!(matches!(error, ResourceError::InvalidRoot { path, .. } if path.as_os_str().is_empty()));
}

#[test]
fn unresolved_executable_or_working_directory_paths_are_rejected() {
    let root = TempDir::new().unwrap();
    let relative_executable = Path::new("wsdist-gui");
    let error = AppPaths::from_parts(None, relative_executable, root.path()).unwrap_err();
    assert!(matches!(error, ResourceError::InvalidRoot { path, .. } if path == relative_executable));

    let relative_directory = Path::new("launch-directory");
    let error = AppPaths::from_parts(
        Some(OsStr::new("assets")),
        &executable_at(root.path()),
        relative_directory,
    )
    .unwrap_err();
    assert!(matches!(error, ResourceError::InvalidRoot { path, .. } if path == relative_directory));
}

#[test]
fn each_missing_required_resource_reports_its_selected_root_path() {
    for relative in REQUIRED_FILES.into_iter().chain(["data/gear"]) {
        let archive = TempDir::new().unwrap();
        populate_resources(archive.path());
        let missing_path = archive.path().join(relative);
        if missing_path.is_dir() {
            fs::remove_dir_all(&missing_path).unwrap();
        } else {
            fs::remove_file(&missing_path).unwrap();
        }
        let paths = AppPaths::from_parts(
            None,
            &executable_at(archive.path()),
            Path::new("unused"),
        )
        .unwrap();
        assert_data_error(load_resources_at(paths), &missing_path);
    }
}

#[test]
fn each_invalid_required_file_reports_its_path_instead_of_a_partial_catalog() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (fragment, _) = resource_support::piece_fragment(&source.join("data/gear"), "Empty").unwrap();
    let mut files = REQUIRED_FILES.map(PathBuf::from).to_vec();
    files.push(fragment.strip_prefix(&source).unwrap().to_path_buf());
    for relative in files {
        let archive = TempDir::new().unwrap();
        populate_resources(archive.path());
        let invalid_path = archive.path().join(&relative);
        let invalid_content = if relative.extension() == Some(OsStr::new("csv")) {
            "not,a,catalog\n"
        } else {
            "{"
        };
        fs::write(&invalid_path, invalid_content).unwrap();
        let paths = AppPaths::from_parts(
            None,
            &executable_at(archive.path()),
            Path::new("unused"),
        )
        .unwrap();
        assert_data_error(load_resources_at(paths), &invalid_path);
    }
}

#[test]
fn optional_icons_and_missing_or_malformed_profiles_do_not_block_resource_loading() {
    let archive = TempDir::new().unwrap();
    populate_resources(archive.path());
    let paths = AppPaths::from_parts(
        None,
        &executable_at(archive.path()),
        Path::new("unused"),
    )
    .unwrap();

    let loaded = load_resources_at(paths.clone()).unwrap();
    assert_eq!(loaded.catalog.by_name2("Empty"), Some(loaded.catalog.empty));
    assert!(!loaded.paths.defaults.exists());
    assert!(!loaded.paths.icons.exists());

    fs::write(&paths.defaults, "{malformed profile").unwrap();
    let loaded = load_resources_at(paths).unwrap();
    assert_eq!(loaded.catalog.by_name2("Empty"), Some(loaded.catalog.empty));
    assert_eq!(fs::read_to_string(loaded.paths.defaults).unwrap(), "{malformed profile");
}

#[cfg(any(unix, windows))]
#[test]
fn native_non_unicode_override_survives_discovery_and_catalog_loading() {
    #[cfg(unix)]
    let component = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"archive-\xff".to_vec())
    };
    #[cfg(windows)]
    let component = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0x61, 0x72, 0x63, 0x68, 0x69, 0x76, 0x65, 0x2d, 0xd800])
    };

    let launch_directory = TempDir::new().unwrap();
    let native_root = launch_directory.path().join(&component);
    populate_resources(&native_root);
    let paths = AppPaths::from_parts(
        Some(component.as_os_str()),
        Path::new("unused"),
        launch_directory.path(),
    )
    .unwrap();
    let loaded = load_resources_at(paths).unwrap();
    assert_eq!(loaded.paths.root.as_os_str(), native_root.as_os_str());
    assert_eq!(loaded.catalog.by_name2("Empty"), Some(loaded.catalog.empty));
}

#[cfg(windows)]
#[test]
fn windows_drive_relative_override_uses_the_injected_drive_directory() {
    let paths = AppPaths::from_parts(
        Some(OsStr::new(r"C:assets")),
        Path::new("unused"),
        Path::new(r"C:\launch\directory"),
    )
    .unwrap();
    assert_eq!(paths.root, Path::new(r"C:\launch\directory\assets"));

    let error = AppPaths::from_parts(
        Some(OsStr::new(r"D:assets")),
        Path::new("unused"),
        Path::new(r"C:\launch\directory"),
    )
    .unwrap_err();
    assert!(matches!(error, ResourceError::InvalidRoot { path, .. } if path == Path::new(r"D:assets")));
}

#[cfg(windows)]
#[test]
fn windows_root_relative_override_keeps_the_injected_drive() {
    let paths = AppPaths::from_parts(
        Some(OsStr::new(r"\assets")),
        Path::new("unused"),
        Path::new(r"C:\launch\directory"),
    )
    .unwrap();
    assert_eq!(paths.root, Path::new(r"C:\assets"));
}

#[test]
fn startup_error_exposes_the_failing_file_and_close_hides_the_window() {
    i_slint_backend_testing::init_no_event_loop();
    let archive = TempDir::new().unwrap();
    let paths = AppPaths::from_parts(
        None,
        &executable_at(archive.path()),
        Path::new("unused"),
    )
    .unwrap();
    let missing_path = archive.path().join("data/gear");
    let error = match load_resources_at(paths) {
        Err(error) => error,
        Ok(_) => panic!("an archive with no data unexpectedly loaded"),
    };
    let window = create_resource_error_window(&error).unwrap();
    window.show().unwrap();
    assert!(window.window().is_visible());

    let details = i_slint_backend_testing::ElementHandle::find_by_accessible_label(
        &window,
        "Resource error details",
    )
    .next()
    .unwrap();
    let displayed_path = missing_path.display().to_string();
    assert!(details.accessible_value().unwrap().contains(displayed_path.as_str()));

    let close = i_slint_backend_testing::ElementHandle::find_by_accessible_label(&window, "Close")
        .next()
        .unwrap();
    close.invoke_accessible_default_action();
    assert!(!window.window().is_visible());
}
