# Bundled frameworks

Place framework resource APKs here so the decoder can resolve framework
references (`@android:string/ok`, `0x01010003`, …) without a device.

- `1.apk` — the stock Android `framework-res.apk` (package id `0x01`). This is
  the default framework used when an APK references `@android:*` resources.

Users can install OEM/version-specific frameworks at runtime with:

```
lancer-apktool install-framework <framework.apk> -t <tag>
```

Installed frameworks are cached per-OS under the app data directory
(`%APPDATA%\lancer-apktool\framework\` on Windows). This `frameworks/` folder is
only the default shipped with the binary.
