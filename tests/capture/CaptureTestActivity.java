package net.mathru.whisker.capturetest;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.view.Gravity;
import android.webkit.WebView;
import android.widget.FrameLayout;
import android.widget.TextView;

/** Hardware-accelerated WebView with a sibling overlay matching the debugger's capture layout. */
public class CaptureTestActivity extends Activity {
    FrameLayout target;
    TextView overlay;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        FrameLayout stage = new FrameLayout(this);
        target = new FrameLayout(this);
        FrameLayout.LayoutParams region = new FrameLayout.LayoutParams(-1, 600);
        region.setMargins(20, 100, 20, 0);
        stage.addView(target, region);
        WebView web = new WebView(this);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setMediaPlaybackRequiresUserGesture(false);
        web.getSettings().setAllowFileAccess(true);
        target.addView(web, new FrameLayout.LayoutParams(-1, -1));
        overlay = new TextView(this);
        overlay.setText("Debugger overlay");
        overlay.setGravity(Gravity.CENTER);
        overlay.setBackgroundColor(Color.MAGENTA);
        FrameLayout.LayoutParams panel = new FrameLayout.LayoutParams(500, 100);
        panel.setMargins(20, 100, 0, 0);
        stage.addView(overlay, panel);
        setContentView(stage);
        web.loadUrl("file:///android_asset/fixture.html");
    }
}
