package net.mathru.whisker.capturetest;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.os.Bundle;
import android.view.View;
import java.io.File;
import java.io.FileOutputStream;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import kotlin.Unit;
import net.mathru.whisker.view_capture.CompositedCaptureKt;

/** Runs the production compositor capture without network, app credentials or additional test dependencies. */
public class CaptureTest extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }

    @Override public void onStart() {
        CaptureTestActivity activity = null;
        Bundle result = new Bundle();
        try {
            activity = (CaptureTestActivity) startActivitySync(new Intent(getTargetContext(), CaptureTestActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final CaptureTestActivity app = activity;
            // Poll rendered frames until the local video has decoded. No network or fixed startup delay.
            Bitmap composite = null;
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(25);
            while (System.nanoTime() < deadline) {
                waitForIdleSync();
                composite = capture(app, 600);
                if (countGreen(composite) > 10000) break;
                composite.recycle();
                composite = null;
                Thread.sleep(100);
            }
            require(composite != null, "composited capture must include decoded video");
            require(countColor(composite, Color.MAGENTA) == 0, "sibling debugger must not appear in the image");
            require(app.overlay.getVisibility() == View.VISIBLE, "sibling debugger must be restored");
            save(composite, "composited.png");

            AtomicReference<Bitmap> software = new AtomicReference<>();
            runOnMainSync(() -> {
                Bitmap bitmap = Bitmap.createBitmap(app.target.getWidth(), 600, Bitmap.Config.ARGB_8888);
                app.target.draw(new Canvas(bitmap));
                software.set(bitmap);
            });
            require(countGreen(software.get()) == 0, "fixture must reproduce software capture omitting video");
            save(software.get(), "software.png");
            software.get().recycle();

            Bitmap cropped = capture(app, 300);
            require(cropped.getWidth() == composite.getWidth() && cropped.getHeight() == 300, "crop must retain physical dimensions");
            require(countGreen(cropped) > 10000, "crop must include the visible video region");
            require(countColor(cropped, Color.MAGENTA) == 0, "repeated capture must still exclude siblings");
            require(app.overlay.getVisibility() == View.VISIBLE, "repeated capture must restore siblings");
            cropped.recycle();
            composite.recycle();

            AtomicReference<Throwable> rejected = new AtomicReference<>();
            runOnMainSync(() -> {
                Bitmap bitmap = Bitmap.createBitmap(app.target.getWidth(), 600, Bitmap.Config.ARGB_8888);
                app.target.setTranslationY(-1000);
                try {
                    CompositedCaptureKt.captureComposited(app.target, bitmap, error -> Unit.INSTANCE);
                } catch (IllegalStateException expected) { rejected.set(expected); }
                finally { app.target.setTranslationY(0); bitmap.recycle(); }
            });
            require(rejected.get() != null, "off-screen capture must fail rather than return a misleading image");
            require(app.overlay.getVisibility() == View.VISIBLE, "failed capture must leave siblings visible");
            CountDownLatch timedOut = new CountDownLatch(1);
            AtomicReference<Throwable> timeoutFailure = new AtomicReference<>();
            AtomicReference<Bitmap> pending = new AtomicReference<>();
            runOnMainSync(() -> {
                Bitmap bitmap = Bitmap.createBitmap(app.target.getWidth(), 600, Bitmap.Config.ARGB_8888);
                pending.set(bitmap);
                CompositedCaptureKt.captureComposited(app.target, bitmap, error -> {
                    timeoutFailure.set(error);
                    timedOut.countDown();
                    return Unit.INSTANCE;
                });
                // Prevent a draw after the request has hidden the siblings.
                app.getWindow().getDecorView().setVisibility(View.INVISIBLE);
            });
            try {
                require(timedOut.await(5, TimeUnit.SECONDS), "missing frame must complete with a timeout");
                require(timeoutFailure.get() != null, "missing frame must report an error");
                require(app.overlay.getVisibility() == View.VISIBLE, "timeout must restore hidden siblings");
            } finally {
                runOnMainSync(() -> app.getWindow().getDecorView().setVisibility(View.VISIBLE));
                pending.get().recycle();
            }
            result.putString("result", "PASS: WebView video, sibling exclusion/restoration, repeat, crop, off-screen rejection, timeout cleanup");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("error", error.toString());
            finish(Activity.RESULT_CANCELED, result);
        } finally {
            if (activity != null) {
                final CaptureTestActivity app = activity;
                runOnMainSync(app::finish);
            }
        }
    }

    private Bitmap capture(CaptureTestActivity activity, int height) throws Exception {
        AtomicReference<Bitmap> image = new AtomicReference<>();
        AtomicReference<Throwable> failure = new AtomicReference<>();
        CountDownLatch completed = new CountDownLatch(1);
        runOnMainSync(() -> {
            Bitmap bitmap = Bitmap.createBitmap(activity.target.getWidth(), height, Bitmap.Config.ARGB_8888);
            image.set(bitmap);
            try {
                CompositedCaptureKt.captureComposited(activity.target, bitmap, error -> {
                    failure.set(error);
                    completed.countDown();
                    return Unit.INSTANCE;
                });
            } catch (Throwable error) { failure.set(error); completed.countDown(); }
        });
        require(completed.await(10, TimeUnit.SECONDS), "capture callback must complete");
        if (failure.get() != null) throw new AssertionError("capture failed", failure.get());
        return image.get();
    }

    private static int countGreen(Bitmap bitmap) {
        int count = 0;
        for (int y = 0; y < bitmap.getHeight(); y++) for (int x = 0; x < bitmap.getWidth(); x++) {
            int color = bitmap.getPixel(x, y);
            if (Color.green(color) > 150 && Color.red(color) < 80 && Color.blue(color) < 80) count++;
        }
        return count;
    }

    private static int countColor(Bitmap bitmap, int expected) {
        int count = 0;
        for (int y = 0; y < bitmap.getHeight(); y++) for (int x = 0; x < bitmap.getWidth(); x++) {
            if (bitmap.getPixel(x, y) == expected) count++;
        }
        return count;
    }

    private void save(Bitmap image, String name) throws Exception {
        try (FileOutputStream stream = new FileOutputStream(new File(getTargetContext().getFilesDir(), name))) {
            require(image.compress(Bitmap.CompressFormat.PNG, 100, stream), "PNG must be saved");
        }
    }

    private static void require(boolean value, String message) {
        if (!value) throw new AssertionError(message);
    }
}
