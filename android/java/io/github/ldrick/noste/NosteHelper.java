package io.github.ldrick.noste;

import android.app.Activity;
import android.app.Application;
import android.app.Fragment;
import android.content.ActivityNotFoundException;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.graphics.Rect;
import android.graphics.pdf.PdfRenderer;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.util.Log;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityManager;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;

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

    // ---- Accessibility -------------------------------------------------------------------------
    //
    // Slint has no accessibility support on Android, so src/a11y.rs sends the accessible elements
    // as a flat list and they are exposed here as virtual views of the NativeActivity's content
    // view. Actions go back to Rust through onA11yAction.

    private static final int NODE_INTS = 7;
    private static final int KIND_TEXT = 0;
    private static final int KIND_BUTTON = 1;
    private static final int KIND_SWITCH = 2;
    private static final int KIND_CHECKBOX = 3;
    private static final int KIND_EDIT = 4;
    private static final int KIND_SLIDER = 5;
    private static final int KIND_IMAGE = 6;
    private static final int KIND_RADIO = 7;
    private static final int FLAG_ENABLED = 1;
    private static final int FLAG_CHECKABLE = 2;
    private static final int FLAG_CHECKED = 4;
    private static final int NO_NODE = Integer.MIN_VALUE;

    static native void onA11yState(boolean enabled);

    static native void onA11yAction(int id, int action, String text);

    static native void onA11yFocus(int id);

    private View mA11yHost;
    private int[] mNodes = new int[0];
    private String[] mTexts = new String[0];
    private int mFocused = NO_NODE;
    private int mHovered = NO_NODE;

    public void installAccessibility() {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                ViewGroup content = mActivity.findViewById(android.R.id.content);
                if (content == null || content.getChildCount() == 0) {
                    Log.w("noste", "no content view for accessibility");
                    return;
                }
                mA11yHost = content.getChildAt(0);
                // Slint's input view lies on top of the content view and has nothing to say.
                for (int i = 1; i < content.getChildCount(); i++) {
                    content.getChildAt(i).setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
                }
                mA11yHost.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
                // The window's action bar lies hidden under the native surface; TalkBack must not read it.
                if (mActivity.getActionBar() != null) {
                    mActivity.getActionBar().hide();
                }
                final AccessibilityNodeProvider provider = new NodeProvider();
                mA11yHost.setAccessibilityDelegate(new View.AccessibilityDelegate() {
                    @Override
                    public AccessibilityNodeProvider getAccessibilityNodeProvider(View host) {
                        return provider;
                    }
                });
                // Slint consumes hover moves, but hover enter/exit reach the view, so touching
                // the screen in explore-by-touch mode focuses the node under the finger.
                mA11yHost.setOnHoverListener(new View.OnHoverListener() {
                    @Override
                    public boolean onHover(View v, MotionEvent event) {
                        return hover(event);
                    }
                });
                final AccessibilityManager manager =
                        (AccessibilityManager) mActivity.getSystemService(Context.ACCESSIBILITY_SERVICE);
                // Any accessibility service (TalkBack, Switch Access, Voice Access, UI tests).
                manager.addAccessibilityStateChangeListener(new AccessibilityManager.AccessibilityStateChangeListener() {
                    @Override
                    public void onAccessibilityStateChanged(boolean enabled) {
                        onA11yState(enabled);
                    }
                });
                onA11yState(manager.isEnabled());
            }
        });
    }

    /** {@code nodes} holds NODE_INTS ints per node, {@code texts} label and value per node. */
    public void updateAccessibility(final int[] nodes, final String texts, final boolean reset) {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (mA11yHost == null) {
                    return;
                }
                int focusedIndex = indexOf(mFocused);
                String focusedText = focusedIndex >= 0 ? mTexts[focusedIndex * 2] + mTexts[focusedIndex * 2 + 1] : null;
                mNodes = nodes;
                mTexts = nodes.length == 0 ? new String[0] : texts.split("\u001f", -1);
                if (indexOf(mFocused) < 0) {
                    mFocused = NO_NODE;
                }
                if (indexOf(mHovered) < 0) {
                    mHovered = NO_NODE;
                }
                AccessibilityEvent changed = newEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
                changed.setContentChangeTypes(AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE);
                changed.setSource(mA11yHost);
                send(changed);
                if (reset && mNodes.length > 0) {
                    focus(mNodes[0]);
                } else if (mFocused != NO_NODE && focusedText != null) {
                    int i = indexOf(mFocused);
                    if (!focusedText.equals(mTexts[i * 2] + mTexts[i * 2 + 1])) {
                        AccessibilityEvent event = nodeEvent(mFocused, AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
                        event.setContentChangeTypes(AccessibilityEvent.CONTENT_CHANGE_TYPE_TEXT);
                        send(event);
                    }
                }
            }
        });
    }

    @SuppressWarnings("deprecation")
    public void announce(final String text) {
        mActivity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (mA11yHost != null) {
                    mA11yHost.announceForAccessibility(text);
                }
            }
        });
    }

    private int indexOf(int id) {
        if (id == NO_NODE) {
            return -1;
        }
        for (int i = 0; i < mNodes.length / NODE_INTS; i++) {
            if (mNodes[i * NODE_INTS] == id) {
                return i;
            }
        }
        return -1;
    }

    private boolean hover(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_HOVER_ENTER:
            case MotionEvent.ACTION_HOVER_MOVE: {
                int id = nodeAt((int) event.getX(), (int) event.getY());
                if (id != mHovered) {
                    int old = mHovered;
                    mHovered = id;
                    if (id != NO_NODE) {
                        send(nodeEvent(id, AccessibilityEvent.TYPE_VIEW_HOVER_ENTER));
                    }
                    if (old != NO_NODE) {
                        send(nodeEvent(old, AccessibilityEvent.TYPE_VIEW_HOVER_EXIT));
                    }
                }
                return true;
            }
            case MotionEvent.ACTION_HOVER_EXIT:
                if (mHovered != NO_NODE) {
                    send(nodeEvent(mHovered, AccessibilityEvent.TYPE_VIEW_HOVER_EXIT));
                    mHovered = NO_NODE;
                }
                return true;
            default:
                return false;
        }
    }

    /** The smallest node at x, y (view coordinates). */
    private int nodeAt(int x, int y) {
        int best = NO_NODE;
        long bestArea = Long.MAX_VALUE;
        for (int i = 0; i < mNodes.length / NODE_INTS; i++) {
            int o = i * NODE_INTS;
            if (x >= mNodes[o + 1] && x < mNodes[o + 1] + mNodes[o + 3] && y >= mNodes[o + 2]
                    && y < mNodes[o + 2] + mNodes[o + 4]) {
                long area = (long) mNodes[o + 3] * mNodes[o + 4];
                if (area < bestArea) {
                    best = mNodes[o];
                    bestArea = area;
                }
            }
        }
        return best;
    }

    private void focus(int id) {
        if (mFocused == id) {
            return;
        }
        if (mFocused != NO_NODE) {
            send(nodeEvent(mFocused, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED));
        }
        mFocused = id;
        onA11yFocus(id);
        send(nodeEvent(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED));
    }

    @SuppressWarnings("deprecation")
    private AccessibilityEvent newEvent(int type) {
        AccessibilityEvent event = AccessibilityEvent.obtain(type);
        event.setPackageName(mActivity.getPackageName());
        return event;
    }

    private AccessibilityEvent nodeEvent(int id, int type) {
        AccessibilityEvent event = newEvent(type);
        event.setSource(mA11yHost, id);
        int i = indexOf(id);
        if (i >= 0) {
            event.setClassName(className(mNodes[i * NODE_INTS + 5]));
            event.getText().add(mTexts[i * 2]);
            event.setEnabled((mNodes[i * NODE_INTS + 6] & FLAG_ENABLED) != 0);
        }
        return event;
    }

    private void send(AccessibilityEvent event) {
        ViewParent parent = mA11yHost.getParent();
        AccessibilityManager manager =
                (AccessibilityManager) mActivity.getSystemService(Context.ACCESSIBILITY_SERVICE);
        if (parent != null && manager.isEnabled()) {
            parent.requestSendAccessibilityEvent(mA11yHost, event);
        }
    }

    private static String className(int kind) {
        switch (kind) {
            case KIND_BUTTON:
                return "android.widget.Button";
            case KIND_SWITCH:
                return "android.widget.Switch";
            case KIND_CHECKBOX:
                return "android.widget.CheckBox";
            case KIND_EDIT:
                return "android.widget.EditText";
            case KIND_SLIDER:
                return "android.widget.SeekBar";
            case KIND_IMAGE:
                return "android.widget.ImageView";
            case KIND_RADIO:
                return "android.widget.RadioButton";
            default:
                return "android.widget.TextView";
        }
    }

    private class NodeProvider extends AccessibilityNodeProvider {
        @Override
        @SuppressWarnings("deprecation")
        public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
            View host = mA11yHost;
            if (id == HOST_VIEW_ID) {
                AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host);
                host.onInitializeAccessibilityNodeInfo(info);
                for (int i = 0; i < mNodes.length / NODE_INTS; i++) {
                    info.addChild(host, mNodes[i * NODE_INTS]);
                }
                return info;
            }
            int i = indexOf(id);
            if (i < 0) {
                return null;
            }
            int o = i * NODE_INTS;
            int kind = mNodes[o + 5];
            int flags = mNodes[o + 6];
            String label = mTexts[i * 2];
            String value = mTexts[i * 2 + 1];

            AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host, id);
            info.setPackageName(mActivity.getPackageName());
            info.setClassName(className(kind));
            info.setParent(host);
            Rect bounds = new Rect(mNodes[o + 1], mNodes[o + 2], mNodes[o + 1] + mNodes[o + 3],
                    mNodes[o + 2] + mNodes[o + 4]);
            info.setBoundsInParent(bounds);
            int[] location = new int[2];
            host.getLocationOnScreen(location);
            bounds.offset(location[0], location[1]);
            info.setBoundsInScreen(bounds);
            info.setVisibleToUser(true);
            info.setEnabled((flags & FLAG_ENABLED) != 0);
            if ((flags & FLAG_CHECKABLE) != 0) {
                info.setCheckable(true);
                info.setChecked((flags & FLAG_CHECKED) != 0);
            }
            switch (kind) {
                case KIND_EDIT:
                    info.setEditable(true);
                    info.setFocusable(true);
                    info.setClickable(true);
                    info.setText(value.isEmpty() ? null : value);
                    info.setHintText(label);
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_TEXT);
                    break;
                case KIND_SLIDER:
                    info.setContentDescription(label);
                    if (Build.VERSION.SDK_INT >= 30) {
                        info.setStateDescription(value);
                    } else {
                        info.setContentDescription(label + ", " + value);
                    }
                    info.setScrollable(true);
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                    break;
                case KIND_TEXT:
                    info.setText(label);
                    break;
                case KIND_IMAGE:
                    info.setContentDescription(label);
                    break;
                default:
                    info.setText(label);
                    info.setClickable(true);
                    info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
                    break;
            }
            boolean focused = id == mFocused;
            info.setAccessibilityFocused(focused);
            info.addAction(focused ? AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS
                    : AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
            return info;
        }

        @Override
        public boolean performAction(int id, int action, Bundle arguments) {
            if (id == HOST_VIEW_ID) {
                return mA11yHost.performAccessibilityAction(action, arguments);
            }
            if (indexOf(id) < 0) {
                return false;
            }
            switch (action) {
                case AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS:
                    focus(id);
                    return true;
                case AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS:
                    if (mFocused == id) {
                        mFocused = NO_NODE;
                        onA11yFocus(NO_NODE);
                        send(nodeEvent(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED));
                    }
                    return true;
                case AccessibilityNodeInfo.ACTION_CLICK:
                    onA11yAction(id, action, null);
                    send(nodeEvent(id, AccessibilityEvent.TYPE_VIEW_CLICKED));
                    return true;
                case AccessibilityNodeInfo.ACTION_SCROLL_FORWARD:
                case AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD:
                    onA11yAction(id, action, null);
                    return true;
                case AccessibilityNodeInfo.ACTION_SET_TEXT: {
                    CharSequence text = arguments == null ? null
                            : arguments.getCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE);
                    onA11yAction(id, action, text == null ? "" : text.toString());
                    return true;
                }
                default:
                    return false;
            }
        }
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
