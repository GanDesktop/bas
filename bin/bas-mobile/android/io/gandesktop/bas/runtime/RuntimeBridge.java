package io.gandesktop.bas.runtime;

import android.content.Context;
import android.content.Intent;
import org.json.JSONObject;

/** Small JNI boundary; the Android service owns the process beyond the Activity. */
public final class RuntimeBridge {
    private final Context context;

    public RuntimeBridge(Context context) {
        this.context = context.getApplicationContext();
        initialize();
    }

    private native void initialize();

    public String call(String action) throws Exception {
        if (action.equals("start")) {
            if (LocalService.phase.equals("stopping")) throw new IllegalStateException("BAS is still stopping");
            context.startForegroundService(new Intent(context, LocalService.class));
        } else if (action.equals("stop")) {
            context.stopService(new Intent(context, LocalService.class));
        } else if (!action.equals("status")) {
            throw new IllegalArgumentException("Unknown runtime action");
        }
        JSONObject status = new JSONObject();
        status.put("available", RuntimeBundle.available(context));
        status.put("phase", LocalService.phase);
        status.put("error", LocalService.error);
        status.put("endpoint", "unix://" + RuntimeBundle.socket(context).getAbsolutePath());
        return status.toString();
    }
}
