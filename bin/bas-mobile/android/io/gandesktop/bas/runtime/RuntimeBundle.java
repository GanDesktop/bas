package io.gandesktop.bas.runtime;

import android.content.Context;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.Locale;
import java.nio.charset.StandardCharsets;
import android.system.Os;
import org.json.JSONObject;

final class RuntimeBundle {
    static File base(Context context) { return new File(context.getFilesDir(), "bas-runtime"); }
    static File run(Context context) { return new File(base(context), "run"); }
    static File browser(Context context) { return new File(run(context), "browser"); }
    static File socket(Context context) { return new File(run(context), "service.sock"); }

    static void identity(File root, File run) throws Exception {
        int uid = Os.getuid();
        int gid = Os.getgid();
        accountFile(root, run, "passwd", uid, "bas:x:" + uid + ":" + gid + ":BAS:/home/bas:/bin/sh");
        accountFile(root, run, "group", gid, "bas:x:" + gid + ":");
    }

    private static void accountFile(File root, File run, String name, int id, String entry) throws Exception {
        var lines = Files.readAllLines(new File(root, "etc/" + name).toPath(), StandardCharsets.UTF_8);
        lines.removeIf(line -> {
            String[] fields = line.split(":");
            return fields.length > 2 && (fields[0].equals("bas") || fields[2].equals(Integer.toString(id)));
        });
        lines.add(entry);
        File target = new File(run, name);
        Files.write(target.toPath(), lines, StandardCharsets.UTF_8);
        Os.chmod(target.getPath(), 0600);
    }

    static boolean available(Context context) {
        try (var input = context.getAssets().open("bas-runtime/manifest.json")) { return true; }
        catch (IOException error) { return false; }
    }

    static File install(Context context) throws Exception {
        JSONObject manifest;
        try (var input = context.getAssets().open("bas-runtime/manifest.json")) {
            manifest = new JSONObject(new String(read(input), java.nio.charset.StandardCharsets.UTF_8));
        }
        String digest = manifest.getString("sha256");
        if (!digest.matches("[a-f0-9]{64}")) throw new IOException("Invalid runtime digest");
        File versions = new File(base(context), "versions");
        File installed = new File(versions, digest);
        if (new File(installed, ".installed").isFile()) return installed;
        File staging = new File(versions, digest + ".installing");
        remove(staging);
        Files.createDirectories(staging.toPath());
        File archive = new File(base(context), "runtime.tar.gz");
        MessageDigest hash = MessageDigest.getInstance("SHA-256");
        try (var input = context.getAssets().open("bas-runtime/rootfs.tar.gz");
             var output = Files.newOutputStream(archive.toPath())) {
            byte[] buffer = new byte[65536];
            int count;
            while ((count = input.read(buffer)) != -1) {
                hash.update(buffer, 0, count);
                output.write(buffer, 0, count);
            }
        }
        StringBuilder actual = new StringBuilder();
        for (byte b : hash.digest()) actual.append(String.format(Locale.ROOT, "%02x", b & 255));
        if (!digest.contentEquals(actual)) throw new IOException("Runtime checksum mismatch");
        File log = new File(base(context), "install.log");
        Process unpack = new ProcessBuilder("/system/bin/toybox", "tar", "-xozf",
                archive.getPath(), "-C", staging.getPath())
                .redirectErrorStream(true).redirectOutput(log).start();
        if (unpack.waitFor() != 0) throw new IOException("Runtime extraction failed; see " + log);
        Files.write(new File(staging, ".installed").toPath(), digest.getBytes(java.nio.charset.StandardCharsets.UTF_8));
        remove(installed);
        Files.move(staging.toPath(), installed.toPath());
        Files.delete(archive.toPath());
        // User data lives outside versioned root filesystems.
        for (File old : versions.listFiles()) if (!old.equals(installed)) remove(old);
        return installed;
    }

    static byte[] read(java.io.InputStream input) throws IOException {
        java.io.ByteArrayOutputStream output = new java.io.ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int count;
        while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
        return output.toByteArray();
    }

    static void remove(File path) throws IOException {
        if (Files.isSymbolicLink(path.toPath())) { Files.delete(path.toPath()); return; }
        if (path.isDirectory()) {
            File[] children = path.listFiles();
            if (children == null) throw new IOException("Cannot list " + path);
            for (File child : children) remove(child);
        }
        Files.deleteIfExists(path.toPath());
    }
}
