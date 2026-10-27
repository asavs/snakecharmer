"""Source-identity regressions; fixtures are local and no network is used."""

import hashlib
import importlib.util
import io
import pathlib
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "wishlist", pathlib.Path(__file__).with_name("wishlist-from-openrazer.py"))
wishlist = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wishlist)


def blob_id(data):
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()


class SourceIdentityTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="snakecharmer-wishlist-")
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.files = {name: f"fixture for {name}\n".encode() for name in wishlist.SOURCES}
        snapshot = {"files": {name: {"git_blob_sha1": blob_id(data)}
                              for name, data in self.files.items()}}
        self.snapshot = patch.object(wishlist, "SNAPSHOT", snapshot)
        self.snapshot.start()
        self.addCleanup(self.snapshot.stop)
        for name, data in self.files.items():
            (self.root / name).write_bytes(data)

    def test_download_urls_use_a_full_immutable_revision(self):
        self.assertRegex(wishlist.REVISION, r"^[0-9a-f]{40}$")
        for url in wishlist.SOURCES.values():
            self.assertIn(f"/{wishlist.REVISION}/", url)
            self.assertNotIn("/master/", url)

    def test_exact_offline_blobs_are_accepted_without_network(self):
        with patch.object(wishlist.urllib.request, "urlopen",
                          side_effect=AssertionError("offline fetch attempted network")):
            self.assertEqual(wishlist.fetch(self.root),
                             {name: data.decode() for name, data in self.files.items()})

    def test_edited_or_newline_converted_offline_source_is_rejected(self):
        name = next(iter(self.files))
        for data in [self.files[name] + b"local edit", self.files[name].replace(b"\n", b"\r\n")]:
            with self.subTest(data=data):
                (self.root / name).write_bytes(data)
                with self.assertRaisesRegex(ValueError, "does not match OpenRazer"):
                    wishlist.fetch(self.root)

    def test_downloaded_content_is_also_checked_before_parsing(self):
        responses = [io.BytesIO(data) for data in self.files.values()]
        with patch.object(wishlist.urllib.request, "urlopen", side_effect=responses):
            self.assertEqual(wishlist.fetch(None),
                             {name: data.decode() for name, data in self.files.items()})
        with patch.object(wishlist.urllib.request, "urlopen", return_value=io.BytesIO(b"wrong revision")):
            with self.assertRaisesRegex(ValueError, "does not match OpenRazer"):
                wishlist.fetch(None)

    def test_check_mode_never_rewrites_a_stale_document(self):
        docs = self.root / "docs"
        docs.mkdir()
        path = docs / "DEVICE-WISHLIST.md"
        original = "before<!-- BEGIN GENERATED TABLES -->old<!-- END GENERATED TABLES -->after\n"
        path.write_text(original, encoding="utf-8")
        with patch.object(wishlist, "ROOT", self.root), \
                patch.object(wishlist, "parse_devices", return_value=[]), \
                patch.object(wishlist, "parse_driver", return_value=({}, {})), \
                patch("sys.argv", ["wishlist", "--offline", str(self.root), "--check"]):
            self.assertEqual(wishlist.main(), 1)
        self.assertEqual(path.read_text(encoding="utf-8"), original)

    def test_check_mode_accepts_exact_generated_content_without_writing(self):
        docs = self.root / "docs"
        docs.mkdir()
        path = docs / "DEVICE-WISHLIST.md"
        original = ("before<!-- BEGIN GENERATED TABLES -->\n\n" + wishlist.source_note()
                    + wishlist.render([], {}, {}) + "<!-- END GENERATED TABLES -->after\n")
        path.write_text(original, encoding="utf-8")
        with patch.object(wishlist, "ROOT", self.root), \
                patch.object(wishlist, "parse_devices", return_value=[]), \
                patch.object(wishlist, "parse_driver", return_value=({}, {})), \
                patch.object(pathlib.Path, "write_text",
                             side_effect=AssertionError("check mode attempted a write")), \
                patch("sys.argv", ["wishlist", "--offline", str(self.root), "--check"]):
            self.assertEqual(wishlist.main(), 0)


if __name__ == "__main__":
    unittest.main()
