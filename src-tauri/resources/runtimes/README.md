Bundled runtimes for DevNest should be placed here when preparing a distributable app.

Expected layout:

- `resources/runtimes/apache/<version>/bin/httpd.exe`
- `resources/runtimes/nginx/<version>/nginx.exe`
- `resources/runtimes/mysql/<version>/bin/mysqld.exe`
- `resources/runtimes/php/<version>/php.exe`

Examples:

- `resources/runtimes/apache/2.4.63/bin/httpd.exe`
- `resources/runtimes/php/8.3.16/php.exe`

Notes:

- DevNest will detect these runtimes as `bundled` during runtime inventory sync.
- Bundled runtimes are read-only from the app perspective; use `Import to Managed Runtime` only for external runtimes already installed on the machine.
- This folder intentionally does not ship binaries in the repo. Actual runtime packages should be added during distribution/build preparation.

Package catalog:

- `resources/runtimes/packages.json` is the internal runtime package manifest for ServBay-style downloads.
- You can override the bundled manifest at runtime by setting `DEVNEST_RUNTIME_MANIFEST_PATH` to another JSON file.

Package manifest shape:

```json
{
  "packages": [
    {
      "id": "php-8.3.16-win-x64",
      "runtimeType": "php",
      "version": "8.3.16",
      "platform": "windows",
      "arch": "x64",
      "displayName": "PHP 8.3.16",
      "downloadUrl": "https://internal.example/devnest/php-8.3.16-win-x64.zip",
      "checksumSha256": "<sha256>",
      "archiveKind": "zip",
      "entryBinary": "php.exe",
      "notes": "Optional helper text"
    }
  ]
}
```
