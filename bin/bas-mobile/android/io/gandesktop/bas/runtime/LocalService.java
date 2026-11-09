package io.gandesktop.bas.runtime;

import android.app.*;
import android.content.Intent;
import android.os.IBinder;
import android.os.PowerManager;
import android.os.SystemClock;
import android.system.Os;
import android.system.OsConstants;
import java.io.File;
import java.nio.file.Files;
import java.util.*;
import java.util.concurrent.*;

/** Owns a PRoot process tree and persistent BAS data, independently of the UI. */
public final class LocalService extends Service {
    private static final String CHANNEL = "bas-local-service";
    private static final int NOTIFICATION = 1001;
    private static final long START_TIMEOUT_MS = 30000;
    static volatile String phase = "stopped";
    static volatile String error = "";
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private volatile boolean stopping;
    private Process process;
    private PowerManager.WakeLock wakeLock;

    @Override public IBinder onBind(Intent intent) { return null; }

    @Override public int onStartCommand(Intent intent, int flags, int id) {
        if (phase.equals("starting") || phase.equals("running")) return START_NOT_STICKY;
        phase = "starting";
        error = "";
        NotificationManager notifications = getSystemService(NotificationManager.class);
        notifications.createNotificationChannel(new NotificationChannel(CHANNEL, "BAS", NotificationManager.IMPORTANCE_LOW));
        Intent open = new Intent(this, com.longbridge.nsh.AppActivity.class);
        PendingIntent content = PendingIntent.getActivity(this, 0, open, PendingIntent.FLAG_IMMUTABLE);
        startForeground(NOTIFICATION, new Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_notify_sync)
                .setContentTitle("BAS")
                .setContentText(zh() ? "本机服务运行中，可在服务连接中停止" : "Local service active; stop in Connections")
                .setContentIntent(content).setOngoing(true).build());
        wakeLock = getSystemService(PowerManager.class).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "bas:local-service");
        wakeLock.acquire();
        worker.execute(this::startRuntime);
        return START_NOT_STICKY;
    }

    private boolean zh() { return getResources().getConfiguration().getLocales().get(0).getLanguage().equals("zh"); }

    private void startRuntime() {
        try {
            File root = RuntimeBundle.install(this);
            if (stopping) return;
            File base = RuntimeBundle.base(this);
            File run = RuntimeBundle.run(this);
            File home = new File(base, "home");
            File temp = new File(getCacheDir(), "bas-proot");
            for (File dir : Arrays.asList(run, home, temp, new File(home, "workspace"))) Files.createDirectories(dir.toPath());
            Os.chmod(run.getPath(), 0700);
            File browser = RuntimeBundle.browser(this);
            RuntimeBundle.remove(browser);
            Files.createDirectories(browser.toPath());
            Os.chmod(browser.getPath(), 0700);
            Files.deleteIfExists(RuntimeBundle.socket(this).toPath());
            Files.deleteIfExists(new File(run, "daemon.pid").toPath());
            Files.deleteIfExists(new File(run, "proot.pid").toPath());
            File settings = new File(home, "daemon.json");
            if (!settings.exists()) Files.write(settings.toPath(),
                "{\"agents\":[{\"id\":\"acp-test-agent\",\"kind\":\"acp\",\"command\":{\"program\":\"/opt/bas/bin/acp-test-agent\",\"args\":[\"--protocol\",\"v1\"]}}],\"acp\":{\"pool_size\":0}}".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            String libs = getApplicationInfo().nativeLibraryDir;
            List<String> command = new ArrayList<>(Arrays.asList("/system/bin/sh", "-c",
                    "echo $$ > proot.pid; exec \"$@\"", "bas-proot", libs + "/libproot.so",
                    "--link2symlink", "-r", root.getPath(), "-b", "/dev", "-b", "/proc",
                    "-b", home + ":/home/bas", "-b", run + ":/run/bas", "-w", "/home/bas",
                    "/usr/bin/env", "-i", "HOME=/home/bas", "USER=bas", "LOGNAME=bas",
                    "PATH=/opt/bas/bin:/usr/local/bin:/usr/bin:/bin", "TMPDIR=/tmp",
                    "BROWSER=/opt/bas/bin/bas-open-url", "BAS_BROWSER_DIRECTORY=/run/bas/browser",
                    "XDG_CONFIG_HOME=/home/bas/.config", "XDG_STATE_HOME=/home/bas/.local/state",
                    "XDG_DATA_HOME=/home/bas/.local/share", "LANG=C.UTF-8",
                    "/bin/sh", "-c", "echo $$ > /run/bas/daemon.pid; exec /opt/bas/bin/bas " +
                    "--listen unix:///run/bas/service.sock --daemon-settings-file /home/bas/daemon.json " +
                    "--localdb-path /home/bas/sessions.sqlite3 --log-backend stdout"));
            ProcessBuilder builder = new ProcessBuilder(command).directory(run).redirectErrorStream(true)
                    .redirectOutput(new File(base, "service.log"));
            Map<String, String> env = builder.environment();
            env.clear();
            env.put("PATH", "/system/bin");
            env.put("LD_LIBRARY_PATH", libs);
            env.put("PROOT_LOADER", libs + "/libproot-loader.so");
            env.put("PROOT_TMP_DIR", temp.getPath());
            env.put("PROOT_NO_SECCOMP", "1");
            process = builder.start();
            long deadline = SystemClock.elapsedRealtime() + START_TIMEOUT_MS;
            while (!stopping && process.isAlive() && !RuntimeBundle.socket(this).exists()
                    && SystemClock.elapsedRealtime() < deadline) Thread.sleep(100);
            if (stopping) { stopRuntime(); return; }
            if (!process.isAlive() || !RuntimeBundle.socket(this).exists())
                throw new IllegalStateException("BAS did not start; see " + new File(base, "service.log"));
            phase = "running";
            Process owned = process;
            new Thread(() -> {
                try {
                    int exit = owned.waitFor();
                    if (!stopping) {
                        error = "BAS exited (" + exit + "); see " + new File(base, "service.log");
                        phase = "error";
                        stopSelf();
                    }
                } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
            }, "bas-wait").start();
        } catch (Exception failure) {
            error = failure.toString();
            phase = "error";
            stopRuntime();
            stopSelf();
        }
    }

    private void signal(String filename, int signal) {
        if (process == null || !process.isAlive()) return;
        try {
            int pid = Integer.parseInt(new String(Files.readAllBytes(new File(RuntimeBundle.run(this), filename).toPath()), java.nio.charset.StandardCharsets.UTF_8).trim());
            Os.kill(pid, signal);
        } catch (Exception ignored) { /* It may already have exited. */ }
    }

    private void stopRuntime() {
        if (process == null) return;
        try {
            signal("daemon.pid", OsConstants.SIGTERM);
            if (!process.waitFor(6, TimeUnit.SECONDS)) {
                signal("proot.pid", OsConstants.SIGQUIT);
                if (!process.waitFor(2, TimeUnit.SECONDS)) process.destroyForcibly();
            }
        } catch (InterruptedException interrupted) {
            process.destroyForcibly();
            Thread.currentThread().interrupt();
        }
        process = null;
    }

    @Override public void onDestroy() {
        stopping = true;
        if (!phase.equals("error")) phase = "stopping";
        worker.execute(() -> {
            stopRuntime();
            if (!phase.equals("error")) phase = "stopped";
            if (wakeLock != null && wakeLock.isHeld()) wakeLock.release();
        });
        worker.shutdown();
        stopForeground(STOP_FOREGROUND_REMOVE);
        super.onDestroy();
    }
}
