# Contract: SQLite catalog schema v1

Owner: plan author. Transcribed VERBATIM by phase 1.1 into `contracts/sqlite/0001_init.sql`, embedded in
`ss-core` with `include_str!`. DB file: `D:\IOT_Tools\data\ScreenshotStudio\catalog.sqlite` (data dir constant,
overridable by env `SS_DATA_DIR` for tests only). All tables for P1-P4 are created now; later phases only fill
them. Any change after 1.1 = a new numbered migration `contracts/sqlite/000N_*.sql` written by the Owner.

Rules: times are Unix epoch milliseconds (INTEGER). Paths stored relative to their root with `/` separators.
No stored derived fields: `rel_path` and `aspect` are VIRTUAL generated columns.

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version VALUES (1);

CREATE TABLE library_root (
  id             INTEGER PRIMARY KEY,
  path           TEXT NOT NULL UNIQUE,          -- absolute, as the user picked it
  protected_root INTEGER NOT NULL DEFAULT 0,    -- 1 when under a OneDrive folder
  added_at       INTEGER NOT NULL
);

CREATE TABLE image (
  id            INTEGER PRIMARY KEY,
  root_id       INTEGER NOT NULL REFERENCES library_root(id) ON DELETE CASCADE,
  folder        TEXT NOT NULL,                  -- '' for the root itself, else 'a/b'
  file_name     TEXT NOT NULL,
  rel_path      TEXT GENERATED ALWAYS AS (CASE WHEN folder = '' THEN file_name ELSE folder || '/' || file_name END) VIRTUAL,
  size_bytes    INTEGER NOT NULL,
  mtime_ms      INTEGER NOT NULL,
  created_ms    INTEGER,
  taken_ms      INTEGER NOT NULL,               -- filename timestamp > created > mtime
  width         INTEGER,
  height        INTEGER,
  aspect        TEXT GENERATED ALWAYS AS (CASE
                  WHEN width IS NULL OR height IS NULL OR height = 0 THEN NULL
                  WHEN width * 1.0 / height < 0.75 THEN 'phone'
                  WHEN width * 1.0 / height <= 1.05 AND width * 1.0 / height >= 0.95 THEN 'square'
                  WHEN width * 1.0 / height > 2.1 THEN 'wide'
                  WHEN width * 1.0 / height >= 1.3 THEN 'pc'
                  ELSE 'other' END) VIRTUAL,
  content_hash  TEXT,                           -- blake3 hex
  dhash         INTEGER,                        -- 64-bit, stored as signed i64
  phash         INTEGER,
  blur_score    REAL,                           -- variance of Laplacian on the 320 thumb
  dominant_rgb  TEXT,                           -- '#RRGGBB' from sidecar color.dominant
  thumb_state   TEXT NOT NULL DEFAULT 'pending' CHECK (thumb_state IN ('pending','ok','error','offline')),
  cloud_state   TEXT NOT NULL DEFAULT 'unknown' CHECK (cloud_state IN ('local','cloudOnly','unknown')),
  favorite      INTEGER NOT NULL DEFAULT 0,
  rating        INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
  note          TEXT,
  trashed_at    INTEGER,
  missing       INTEGER NOT NULL DEFAULT 0,     -- file vanished since last scan
  indexed_at    INTEGER NOT NULL,
  UNIQUE (root_id, folder, file_name)
);
CREATE INDEX idx_image_taken ON image(taken_ms);
CREATE INDEX idx_image_hash  ON image(content_hash);
CREATE INDEX idx_image_dhash ON image(dhash);
CREATE INDEX idx_image_trash ON image(trashed_at);

CREATE VIRTUAL TABLE text_fts USING fts5(
  file_name, ocr_text, note,
  tokenize = 'unicode61 remove_diacritics 2'
);                                              -- rowid = image.id, maintained by ss-core::catalog (no triggers)

CREATE TABLE ocr_result (
  image_id  INTEGER PRIMARY KEY REFERENCES image(id) ON DELETE CASCADE,
  text      TEXT NOT NULL,
  lines_json TEXT NOT NULL,                     -- [{text, box, score}]
  engine    TEXT NOT NULL,
  done_at   INTEGER NOT NULL
);

CREATE TABLE embedding (
  image_id  INTEGER PRIMARY KEY REFERENCES image(id) ON DELETE CASCADE,
  model     TEXT NOT NULL,
  dim       INTEGER NOT NULL,
  vec       BLOB NOT NULL,                      -- dim * float32 LE, L2-normalized
  done_at   INTEGER NOT NULL
);

CREATE TABLE category (
  image_id  INTEGER NOT NULL REFERENCES image(id) ON DELETE CASCADE,
  label     TEXT NOT NULL,
  score     REAL NOT NULL,
  model     TEXT NOT NULL,
  PRIMARY KEY (image_id, label)
);

CREATE TABLE tag (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE COLLATE NOCASE,
  color TEXT NOT NULL
);
CREATE TABLE image_tag (
  image_id INTEGER NOT NULL REFERENCES image(id) ON DELETE CASCADE,
  tag_id   INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (image_id, tag_id)
);

CREATE TABLE album (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE album_image (
  album_id INTEGER NOT NULL REFERENCES album(id) ON DELETE CASCADE,
  image_id INTEGER NOT NULL REFERENCES image(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  PRIMARY KEY (album_id, image_id)
);

CREATE TABLE smart_filter (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE,
  query_json TEXT NOT NULL,                     -- ImageQuery JSON (contract-tauri-ipc.md)
  created_at INTEGER NOT NULL
);

CREATE TABLE op_journal (
  id         INTEGER PRIMARY KEY,
  batch_id   INTEGER NOT NULL,
  op         TEXT NOT NULL CHECK (op IN ('move','copy','rename','trash','restore','recycle','purge','tag','untag',
                                         'favorite','rating','note','albumAdd','albumRemove','albumReorder',
                                         'editOverwrite','convert')),
  image_id   INTEGER,
  payload_json TEXT NOT NULL,                   -- everything undo needs: old/new paths, old values, recycle item id, backup path
  undoable   INTEGER NOT NULL,                  -- 0 only for 'purge'; editOverwrite/convert keep a backup file (path in payload_json)
  created_at INTEGER NOT NULL,
  undone_at  INTEGER
);
CREATE INDEX idx_journal_batch ON op_journal(batch_id);

CREATE TABLE setting (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL
);
```

Thumbnails are files, not rows: `<data>\thumbs\<id % 256 as 2 hex>\<id>_<w>.webp` (w = 320 | 1280); `thumb_state`
records the state, the file path is computed (never stored).

From phase 2.1 (README Deferred decision #29, owner 2026-11-23): thumbnails are **lossy WebP quality 80** (crate
`webp`); only the 320 file is generated by the index and `thumb_state` describes the 320 file alone; the 1280 file is
generated on its first `ssimg` request and then kept (a cache, never a background pass). Files written before 2.1
(lossless, both sizes) are regenerated once by the first 2.1 index: the marker file `<data>\thumbs\FORMAT` holds the
format in use (`webp-q80`); when it is absent or different, the index deletes the old files, marks every `ok`
thumbnail `pending`, then writes the marker (not a `setting` key, so `SettingKey` is unchanged).
