package com.longbridge.nsh;

import android.content.res.Resources;
import android.os.Build;
import org.json.JSONException;
import org.json.JSONObject;
import java.util.Locale;

/** Maps Android's wallpaper palette to the semantic tokens used by the Rust UI. */
final class DynamicTheme {
    private final NshView view;
    private String applied;

    DynamicTheme(NshView view) {
        this.view = view;
    }

    void refresh() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return;
        try {
            JSONObject modes = new JSONObject()
                    .put("light", palette(false))
                    .put("dark", palette(true));
            String json = new JSONObject().put("cssVars", modes).toString();
            if (!json.equals(applied)) {
                view.setThemeOverrides(json);
                applied = json;
            }
        } catch (JSONException error) {
            throw new IllegalStateException("Cannot encode Android theme", error);
        }
    }

    // Android's resource suffix is inverse tone: _600 = tone 40, _200 = tone 80.
    // https://developer.android.com/develop/ui/views/theming/dynamic-colors
    private JSONObject palette(boolean dark) throws JSONException {
        Resources resources = view.getResources();
        JSONObject colors = new JSONObject();
        for (Role role : ROLES) {
            int color = resources.getColor(dark ? role.dark : role.light, view.getContext().getTheme());
            colors.put(role.token, String.format(Locale.ROOT, "#%06x", color & 0xffffff));
        }
        return colors;
    }

    private record Role(String token, int light, int dark) {}

    private static final Role[] ROLES = {
        new Role("background", android.R.color.system_neutral1_10, android.R.color.system_neutral1_900),
        new Role("foreground", android.R.color.system_neutral1_900, android.R.color.system_neutral1_100),
        new Role("card", android.R.color.system_neutral1_50, android.R.color.system_neutral1_800),
        new Role("card-foreground", android.R.color.system_neutral1_900, android.R.color.system_neutral1_100),
        new Role("popover", android.R.color.system_neutral1_0, android.R.color.system_neutral1_800),
        new Role("popover-foreground", android.R.color.system_neutral1_900, android.R.color.system_neutral1_100),
        new Role("primary", android.R.color.system_accent1_600, android.R.color.system_accent1_200),
        new Role("primary-foreground", android.R.color.system_accent1_0, android.R.color.system_accent1_800),
        new Role("secondary", android.R.color.system_accent2_100, android.R.color.system_accent2_700),
        new Role("secondary-foreground", android.R.color.system_accent2_900, android.R.color.system_accent2_100),
        new Role("muted", android.R.color.system_neutral2_100, android.R.color.system_neutral2_800),
        new Role("muted-foreground", android.R.color.system_neutral2_700, android.R.color.system_neutral2_200),
        new Role("accent", android.R.color.system_accent2_100, android.R.color.system_accent2_700),
        new Role("accent-foreground", android.R.color.system_accent2_900, android.R.color.system_accent2_100),
        new Role("border", android.R.color.system_neutral2_300, android.R.color.system_neutral2_600),
        new Role("input", android.R.color.system_neutral2_500, android.R.color.system_neutral2_500),
        new Role("ring", android.R.color.system_accent1_600, android.R.color.system_accent1_200),
        new Role("sidebar", android.R.color.system_neutral1_50, android.R.color.system_neutral1_800),
        new Role("sidebar-foreground", android.R.color.system_neutral1_900, android.R.color.system_neutral1_100),
        new Role("sidebar-primary", android.R.color.system_accent1_600, android.R.color.system_accent1_200),
        new Role("sidebar-primary-foreground", android.R.color.system_accent1_0, android.R.color.system_accent1_800),
        new Role("sidebar-accent", android.R.color.system_accent2_100, android.R.color.system_accent2_700),
        new Role("sidebar-accent-foreground", android.R.color.system_accent2_900, android.R.color.system_accent2_100),
        new Role("sidebar-border", android.R.color.system_neutral2_300, android.R.color.system_neutral2_600),
        new Role("sidebar-ring", android.R.color.system_accent1_600, android.R.color.system_accent1_200),
    };
}
