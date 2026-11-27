package rs.masamune.logger.firebase;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.net.Uri;
import com.google.firebase.analytics.FirebaseAnalytics;

public final class AnalyticsStartupProvider extends ContentProvider {
    @Override
    public boolean onCreate() {
        // The app registers this provider ahead of FirebaseInitProvider (initOrder 100).
        FirebaseAnalytics.getInstance(getContext()).setAnalyticsCollectionEnabled(false);
        return true;
    }

    @Override
    public String getType(Uri uri) {
        return null;
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        return null;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
