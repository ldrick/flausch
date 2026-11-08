slint::include_modules!();

pub fn run() -> Result<(), slint::PlatformError> {
    App::new()?.run()
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).unwrap();
    run().unwrap();
}
