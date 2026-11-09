// JNI needs unsafe; every block must explain why it is sound (clippy::undocumented_unsafe_blocks).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use jni::errors::LogErrorAndDefault;
use jni::objects::{Global, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::jint;
use jni::{Env, EnvUnowned, JavaVM, NativeMethod, jni_sig, jni_str};
use slint::android::AndroidApp;

use super::{PageImage, PickResult};

const DEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/classes.dex"));
const HELPER_CLASS: &str = "io.github.ldrick.noste.NosteHelper";

static APP: OnceLock<AndroidApp> = OnceLock::new();
static HELPER: OnceLock<Global<JObject<'static>>> = OnceLock::new();
type PickCallback = Box<dyn FnOnce(PickResult) + Send>;
static PENDING_PICK: Mutex<Option<PickCallback>> = Mutex::new(None);

/// Must be called once from `android_main` before anything else in this module.
pub fn init(app: AndroidApp) {
    match load_helper(&app) {
        Ok(helper) => {
            let _ = HELPER.set(helper);
        }
        Err(e) => eprintln!("nøste: cannot load the Java helper: {e}"),
    }
    let _ = APP.set(app);
}

fn app() -> &'static AndroidApp {
    APP.get().expect("platform::init was not called")
}

/// Loads `NosteHelper` from the embedded dex and creates an instance for the activity.
fn load_helper(app: &AndroidApp) -> jni::errors::Result<Global<JObject<'static>>> {
    // SAFETY: android-activity hands out a valid pointer to the process JavaVM.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        // SAFETY: android-activity holds a global reference to the activity for the app's lifetime;
        // this wrapper doesn't own it and only lives for this closure.
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        // SAFETY: DEX is 'static and never written to.
        let dex = unsafe { env.new_direct_byte_buffer(DEX.as_ptr().cast_mut(), DEX.len()) }?;
        let parent = env
            .call_method(
                &activity,
                jni_str!("getClassLoader"),
                jni_sig!("()Ljava/lang/ClassLoader;"),
                &[],
            )?
            .l()?;
        let loader = env.new_object(
            jni_str!("dalvik/system/InMemoryDexClassLoader"),
            jni_sig!("(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V"),
            &[JValue::Object(&dex), JValue::Object(&parent)],
        )?;
        let name = env.new_string(HELPER_CLASS)?;
        let class = env
            .call_method(
                &loader,
                jni_str!("loadClass"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                &[JValue::Object(&name)],
            )?
            .l()?;
        let class = env.cast_local::<JClass>(class)?;
        // SAFETY: `on_pdf_picked` matches the static method `void onPdfPicked(int, String)`, and
        // its second parameter is the class.
        unsafe {
            env.register_native_methods(
                &class,
                &[NativeMethod::from_raw_parts(
                    jni_str!("onPdfPicked"),
                    jni_str!("(ILjava/lang/String;)V"),
                    on_pdf_picked as *mut std::ffi::c_void,
                )],
            )
        }?;
        let helper = env.new_object(
            &class,
            jni_sig!("(Landroid/app/Activity;)V"),
            &[JValue::Object(&activity)],
        )?;
        env.new_global_ref(helper)
    })
}

/// Runs `f` with the helper object on the current thread (attached if needed). Pending Java
/// exceptions are cleared and returned as errors.
fn with_helper<T>(
    f: impl FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let helper = HELPER.get().ok_or("Java helper not loaded")?;
    let vm = JavaVM::singleton().map_err(|e| e.to_string())?;
    vm.attach_current_thread(|env| f(env, helper))
        .map_err(|e: jni::errors::Error| e.to_string())
}

pub fn data_dir() -> PathBuf {
    app()
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp/noste"))
}

pub fn system_language() -> Option<String> {
    app().config().language()
}

// Not `AndroidApp::set_window_flags`: it needs a write lock that Slint holds while it dispatches
// input, so calling it from a UI callback deadlocks.
pub fn keep_screen_on(on: bool) {
    let result = with_helper(|env, helper| {
        env.call_method(
            helper,
            jni_str!("setKeepScreenOn"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(on)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("nøste: cannot change keep screen on: {e}");
    }
}

pub fn set_dark_theme(dark: bool) {
    let result = with_helper(|env, helper| {
        env.call_method(
            helper,
            jni_str!("setLightSystemBars"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(!dark)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("nøste: cannot set the system bar colors: {e}");
    }
}

/// Hides the text cursor handles that Slint shows after a text field was focused from code.
pub fn hide_text_handles() {
    let result = with_helper(|env, helper| {
        env.call_method(helper, jni_str!("hideTextHandles"), jni_sig!("()V"), &[])?;
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("nøste: cannot hide the text handles: {e}");
    }
}

/// Opens the system document picker and copies the chosen PDF to `dest`.
/// `done` runs on a Java thread.
#[allow(
    clippy::needless_pass_by_value,
    reason = "same signature as the desktop version"
)]
pub fn pick_pdf(dest: PathBuf, done: impl FnOnce(PickResult) + Send + 'static) {
    *PENDING_PICK.lock().unwrap() = Some(Box::new(done));
    let result = with_helper(|env, helper| {
        let dest = env.new_string(dest.to_string_lossy())?;
        env.call_method(
            helper,
            jni_str!("pickPdf"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&dest)],
        )?;
        Ok(())
    });
    if let Err(e) = result {
        finish_pick(PickResult::Failed(e));
    }
}

fn finish_pick(result: PickResult) {
    let done = PENDING_PICK.lock().unwrap().take();
    if let Some(done) = done {
        done(result);
    }
}

extern "system" fn on_pdf_picked<'local>(
    mut env: EnvUnowned<'local>,
    _class: JClass<'local>,
    status: jint,
    text: JString<'local>,
) {
    let text = env
        .with_env(|env| -> jni::errors::Result<String> {
            if text.is_null() {
                Ok(String::new())
            } else {
                text.try_to_string(env)
            }
        })
        .resolve::<LogErrorAndDefault>();
    finish_pick(match status {
        0 => PickResult::Picked { title: text },
        1 => PickResult::Cancelled,
        _ => PickResult::Failed(text),
    });
}

pub fn page_count(path: &Path) -> Result<u32, String> {
    with_helper(|env, helper| {
        let path = env.new_string(path.to_string_lossy())?;
        let count = env
            .call_method(
                helper,
                jni_str!("pageCount"),
                jni_sig!("(Ljava/lang/String;)I"),
                &[JValue::Object(&path)],
            )?
            .i()?;
        Ok(count.max(0) as u32)
    })
}

pub fn render_page(path: &Path, index: u32, width: u32) -> Result<PageImage, String> {
    let rgba = with_helper(|env, helper| {
        let path = env.new_string(path.to_string_lossy())?;
        let bytes = env
            .call_method(
                helper,
                jni_str!("renderPage"),
                jni_sig!("(Ljava/lang/String;II)[B"),
                &[
                    JValue::Object(&path),
                    JValue::Int(index as jint),
                    JValue::Int(width as jint),
                ],
            )?
            .l()?;
        let bytes = env.cast_local::<JByteArray>(bytes)?;
        env.convert_byte_array(&bytes)
    })?;
    let height = (rgba.len() / 4 / width.max(1) as usize) as u32;
    Ok(PageImage {
        width,
        height,
        rgba,
    })
}
