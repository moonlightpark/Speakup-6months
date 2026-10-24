package com.speakup.sixmonths;

import android.app.Activity;
import android.os.Build;
import android.view.Gravity;
import android.view.View;
import android.webkit.WebChromeClient;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.PopupWindow;

/**
 * The listening lab's web view (YouGlish videos) on Android, laid over the part of the screen the
 * app asks for (src/web.rs). A NativeActivity's window surface belongs to the native code, so a
 * view added to it is never drawn: the web view lives in a popup window (its own surface) instead.
 * Views can only be touched on the UI thread, so every call is posted there. Positions and sizes
 * are in screen pixels (the app draws from the top-left of the screen).
 */
public final class WebPane {
    private static WebView view;
    private static PopupWindow popup;
    private static String url = "";
    /** Run after every page load: on YouGlish it keeps only the video, its controls and captions. */
    private static String script = "";

    private WebPane() {}

    public static void show(final Activity activity, final String pageUrl, final String pageScript,
                            final int x, final int y, final int w, final int h) {
        activity.runOnUiThread(new Runnable() {
            @Override public void run() {
                script = pageScript;
                if (view == null) {
                    view = new WebView(activity);
                    WebSettings s = view.getSettings();
                    s.setJavaScriptEnabled(true);
                    s.setDomStorageEnabled(true);
                    s.setMediaPlaybackRequiresUserGesture(false);
                    view.setWebChromeClient(new WebChromeClient());
                    view.setWebViewClient(new WebViewClient() {
                        @Override public void onPageFinished(WebView v, String u) {
                            v.evaluateJavascript(script, null);
                        }
                    });
                    popup = new PopupWindow(view, w, h, false);
                    popup.setTouchable(true);
                    popup.setClippingEnabled(false);
                    if (Build.VERSION.SDK_INT >= 29) popup.setIsLaidOutInScreen(true);
                }
                if (!popup.isShowing()) {
                    popup.showAtLocation(activity.getWindow().getDecorView(), Gravity.TOP | Gravity.LEFT, x, y);
                }
                popup.update(x, y, w, h);
                if (!pageUrl.equals(url)) {
                    url = pageUrl;
                    view.loadUrl(pageUrl);
                }
            }
        });
    }

    public static void hide(final Activity activity) {
        activity.runOnUiThread(new Runnable() {
            @Override public void run() {
                if (popup != null && popup.isShowing()) popup.dismiss();
            }
        });
    }
}
