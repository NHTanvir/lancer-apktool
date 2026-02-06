# Debug keystore

`lancer-apktool build` signs rebuilt APKs by default. Place the bundled debug
signing key here as `debug.keystore` (PKCS#12 or JKS).

Generating the standard Android debug key:

```
keytool -genkeypair -v -keystore debug.keystore -storepass android \
  -alias androiddebugkey -keypass android -keyalg RSA -keysize 2048 \
  -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
```

Override at runtime with `lancer-apktool sign --keystore <file> --alias <name>
--storepass <pw> --keypass <pw>`, or use the bundled key via `--debug`.
