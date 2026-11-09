/*
 * Adapted from NSH's AppActivity; adds BAS's dynamic theme lifecycle.
 * Copyright 2026 Longbridge <https://longbridge.com>
 * SPDX-License-Identifier: Apache-2.0
 * See android/licenses/NSH.txt.
 */
package com.longbridge.nsh;

import android.app.Activity;
import android.os.Bundle;

/** The NSH Rust app host, with system colors applied before its first frame. */
public final class AppActivity extends Activity {
    private NshView view;
    private DynamicTheme theme;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        DevActivity.edgeToEdge(this);
        view = new NshView(this);
        new io.gandesktop.bas.runtime.RuntimeBridge(this);
        view.useStandaloneLimits();
        theme = new DynamicTheme(view);
        theme.refresh();
        setContentView(view);
        view.handleNotificationIntent(getIntent());
        if (state == null) view.handleUrlSchemeIntent(getIntent());
        view.runRustApp("nsh");
    }

    @Override
    protected void onResume() {
        super.onResume();
        theme.refresh();
    }

    @Override
    public void onConfigurationChanged(android.content.res.Configuration config) {
        super.onConfigurationChanged(config);
        DevActivity.systemBars(this);
        theme.refresh();
    }

    @Override
    protected void onNewIntent(android.content.Intent intent) {
        super.onNewIntent(intent);
        view.handleNotificationIntent(intent);
        view.handleUrlSchemeIntent(intent);
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        if (!view.handleBack()) super.onBackPressed();
    }

    @Override
    protected void onDestroy() {
        view.destroy();
        super.onDestroy();
    }
}
