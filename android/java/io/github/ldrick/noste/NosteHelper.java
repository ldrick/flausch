package io.github.ldrick.noste;

import android.app.Activity;
import android.app.Application;
import android.app.Fragment;
import android.content.ActivityNotFoundException;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.graphics.pdf.PdfRenderer;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsetsController;
import android.view.WindowManager;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.ByteBuffer;

/**
 * Java side of nøste's Android integration. It is compiled to a dex file by build.rs and loaded
 * at runtime with an InMemoryDexClassLoader, because cargo-apk packages no Java code. Native
 * methods are registered from Rust (src/platform/android.rs).
 */
public class NosteHelper {
    static final int STATUS_OK = 0;
    static final int STATUS_CANCELLED = 1;
    static final int STATUS_FAILED = 2;

    private static final String PICKER_TAG = "noste-document-picker";

    private final Activity mActivity;

    public NosteHelper(Activity activity) {
        mActivity = activity;
        // The picker fragment's class cannot be found by the system class loader. Drop the saved
        // fragment state so a recreated activity does not try to instantiate it.
        activity.getApplication().registerActivityLifecycleCallbacks(new Application.ActivityLifecycleCallbacks() {
            @Override
            public void onActivitySaveInstanceState(Activity a, Bundle outState) {
                if (a == mActivity) {
                    outState.remove("android:fragments");
                }
            }

            @Override
            public void onActivityCreated(Activity a, Bundle savedInstanceState) {
            }

            @Override
            public void onActivityStarted(Activity a) {
            }

            @Override
            public void onActivityResumed(Activity a) {
            }

            @Override
            public void onActivityPaused(Activity a) {
            }

            @Override
            public void onActivityStopped(Activity a) {
            }

            @Override
            public void onActivityDestroyed(Activity a) {
            }
        });
    }

    static native void onDocumentDone(int status, String text);

    /**
     * Opens the system document picker for the comma-separated {@code mimeTypes} and copies the
     * chosen document to {@code destPath}.
     */
    public void pickDocument(String mimeTypes, String destPath) {
        showPicker(PickerFragment.create(false, mimeTypes, destPath, null));
    }

    /** Lets the user choose where to save {@code sourcePath}, suggesting {@code name}. */
    public void saveDocument(String sourcePath, String name, String mimeType) {
        showPicker(PickerFragment.create(true, mimeType, sourcePath, name));
    }

    private void showPicker(final PickerFragment fragment) {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                mActivity.getFragmentManager().beginTransaction()
                        .add(fragment, PICKER_TAG)
                        .commitAllowingStateLoss();
            }
        });
    }

    public void setKeepScreenOn(final boolean on) {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (on) {
                    mActivity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                } else {
                    mActivity.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                }
            }
        });
    }

    /** Dark status and navigation bar icons on a light background, light icons otherwise. */
    @SuppressWarnings("deprecation")
    public void setLightSystemBars(final boolean light) {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                View decor = mActivity.getWindow().getDecorView();
                if (Build.VERSION.SDK_INT >= 30) {
                    WindowInsetsController controller = decor.getWindowInsetsController();
                    if (controller != null) {
                        int mask = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                                | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
                        controller.setSystemBarsAppearance(light ? mask : 0, mask);
                    }
                } else {
                    int mask = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
                    int flags = decor.getSystemUiVisibility();
                    decor.setSystemUiVisibility(light ? flags | mask : flags & ~mask);
                }
            }
        });
    }

    /**
     * Hides the text cursor handles after a text field was focused from code, e.g. when a dialog
     * opens. Slint shows them because the last touch (on the button that opened the dialog) was a
     * tap, and hides them once text arrives from the keyboard. This sends the unchanged text through
     * Slint's own {@code SlintAndroidJavaHelper.updateText}, which has that effect. Tapping or
     * selecting text afterwards shows the handles as usual.
     *
     * Must be called after the field was focused: the runnable then runs after Slint passed the
     * current text to its input view.
     */
    public void hideTextHandles() {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    View input = findView(mActivity.getWindow().getDecorView(), "SlintInputView");
                    if (input == null) {
                        return;
                    }
                    Class<?> view = input.getClass();
                    Class<?> helper = Class.forName("SlintAndroidJavaHelper", true, view.getClassLoader());
                    Method update = helper.getMethod("updateText", String.class, int.class, int.class, int.class,
                            int.class);
                    update.invoke(null, field(view, input, "mText"), field(view, input, "mCursorPosition"),
                            field(view, input, "mAnchorPosition"), 0, 0);
                } catch (ReflectiveOperationException | RuntimeException e) {
                    Log.w("noste", "cannot hide the text handles", e);
                }
            }
        });
    }

    private static Object field(Class<?> type, Object object, String name) throws ReflectiveOperationException {
        Field field = type.getDeclaredField(name);
        field.setAccessible(true);
        return field.get(object);
    }

    private static View findView(View view, String className) {
        if (view.getClass().getName().equals(className)) {
            return view;
        }
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int i = 0; i < group.getChildCount(); i++) {
                View found = findView(group.getChildAt(i), className);
                if (found != null) {
                    return found;
                }
            }
        }
        return null;
    }

    public int pageCount(String path) throws IOException {
        try (ParcelFileDescriptor fd = ParcelFileDescriptor.open(new File(path), ParcelFileDescriptor.MODE_READ_ONLY);
                PdfRenderer renderer = new PdfRenderer(fd)) {
            return renderer.getPageCount();
        }
    }

    /** Renders a page on white at the given width and returns RGBA bytes. */
    public byte[] renderPage(String path, int index, int width) throws IOException {
        try (ParcelFileDescriptor fd = ParcelFileDescriptor.open(new File(path), ParcelFileDescriptor.MODE_READ_ONLY);
                PdfRenderer renderer = new PdfRenderer(fd);
                PdfRenderer.Page page = renderer.openPage(index)) {
            int height = Math.max(1, Math.round((float) width * page.getHeight() / page.getWidth()));
            Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
            try {
                bitmap.eraseColor(Color.WHITE);
                page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY);
                ByteBuffer buffer = ByteBuffer.allocate(width * height * 4);
                bitmap.copyPixelsToBuffer(buffer);
                return buffer.array();
            } finally {
                bitmap.recycle();
            }
        }
    }

    static void copyDocument(ContentResolver resolver, Uri uri, String path, boolean save) {
        String title = displayName(resolver, uri);
        try (InputStream in = save ? new FileInputStream(path) : resolver.openInputStream(uri);
                OutputStream out = save ? resolver.openOutputStream(uri, "wt") : new FileOutputStream(path)) {
            if (in == null || out == null) {
                throw new IOException("cannot open " + uri);
            }
            byte[] buffer = new byte[64 * 1024];
            int n;
            while ((n = in.read(buffer)) > 0) {
                out.write(buffer, 0, n);
            }
        } catch (Exception e) {
            onDocumentDone(STATUS_FAILED, e.toString());
            return;
        }
        onDocumentDone(STATUS_OK, title);
    }

    static String displayName(ContentResolver resolver, Uri uri) {
        try (Cursor cursor = resolver.query(uri, new String[] { OpenableColumns.DISPLAY_NAME }, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                String name = cursor.getString(0);
                if (name != null) {
                    return name;
                }
            }
        } catch (Exception e) {
            // Fall back to the URI below.
        }
        String segment = uri.getLastPathSegment();
        return segment != null ? segment : "document";
    }

    /**
     * Headless fragment that receives the picker result, because NativeActivity does not expose
     * onActivityResult. It either opens a document and copies it to the app's file at {@code path}
     * or creates one and copies that file into it.
     */
    public static class PickerFragment extends Fragment {
        private static final int REQUEST_CODE = 4711;
        private static final String ARG_SAVE = "save";
        private static final String ARG_MIME = "mime";
        private static final String ARG_PATH = "path";
        private static final String ARG_NAME = "name";

        public PickerFragment() {
        }

        static PickerFragment create(boolean save, String mimeTypes, String path, String name) {
            PickerFragment fragment = new PickerFragment();
            Bundle args = new Bundle();
            args.putBoolean(ARG_SAVE, save);
            args.putString(ARG_MIME, mimeTypes);
            args.putString(ARG_PATH, path);
            args.putString(ARG_NAME, name);
            fragment.setArguments(args);
            return fragment;
        }

        @Override
        public void onCreate(Bundle savedInstanceState) {
            super.onCreate(savedInstanceState);
            Bundle args = getArguments();
            String[] mimeTypes = args.getString(ARG_MIME).split(",");
            Intent intent;
            if (args.getBoolean(ARG_SAVE)) {
                intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                intent.setType(mimeTypes[0]);
                intent.putExtra(Intent.EXTRA_TITLE, args.getString(ARG_NAME));
            } else {
                intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                if (mimeTypes.length == 1) {
                    intent.setType(mimeTypes[0]);
                } else {
                    intent.setType("*/*");
                    intent.putExtra(Intent.EXTRA_MIME_TYPES, mimeTypes);
                }
            }
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            try {
                startActivityForResult(intent, REQUEST_CODE);
            } catch (ActivityNotFoundException e) {
                remove();
                onDocumentDone(STATUS_FAILED, e.toString());
            }
        }

        @Override
        public void onActivityResult(int requestCode, int resultCode, Intent data) {
            if (requestCode != REQUEST_CODE) {
                return;
            }
            final Uri uri = resultCode == Activity.RESULT_OK && data != null ? data.getData() : null;
            final ContentResolver resolver = getActivity().getContentResolver();
            final String path = getArguments().getString(ARG_PATH);
            final boolean save = getArguments().getBoolean(ARG_SAVE);
            remove();
            if (uri == null) {
                onDocumentDone(STATUS_CANCELLED, null);
                return;
            }
            new Thread(new Runnable() {
                @Override
                public void run() {
                    copyDocument(resolver, uri, path, save);
                }
            }).start();
        }

        private void remove() {
            getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
        }
    }
}
