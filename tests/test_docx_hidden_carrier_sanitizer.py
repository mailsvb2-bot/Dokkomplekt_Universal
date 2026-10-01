from __future__ import annotations

import importlib.util
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "sanitize_docx_hidden_carriers.py"
spec = importlib.util.spec_from_file_location("sanitize_docx_hidden_carriers", SCRIPT)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def _fixture(path: Path) -> bytes:
    document = b'<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>{{org.name}}</w:t></w:r></w:p></w:body></w:document>'
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", b'''<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/customXml/itemProps1.xml" ContentType="application/vnd.openxmlformats-officedocument.customXmlProperties+xml"/></Types>''')
        archive.writestr("_rels/.rels", b'''<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rDoc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>''')
        archive.writestr("word/document.xml", document)
        archive.writestr("word/_rels/document.xml.rels", b'''<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rCustom" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/></Relationships>''')
        archive.writestr("customXml/item1.xml", b"<secret>hidden patient value</secret>")
        archive.writestr("customXml/itemProps1.xml", b"<props/>")
        archive.writestr("customXml/_rels/item1.xml.rels", b'''<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rProps" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps" Target="itemProps1.xml"/></Relationships>''')
    return document


def test_sanitizer_removes_hidden_custom_xml_without_touching_visible_story(tmp_path: Path) -> None:
    source = tmp_path / "unsafe.docx"
    destination = tmp_path / "safe.docx"
    expected_document = _fixture(source)

    report = module.sanitize_docx_hidden_carriers(source, destination)

    assert report["removed_parts"] == [
        "customXml/_rels/item1.xml.rels",
        "customXml/item1.xml",
        "customXml/itemProps1.xml",
    ]
    with zipfile.ZipFile(destination) as archive:
        names = archive.namelist()
        assert not any(name.lower().startswith("customxml/") for name in names)
        assert archive.read("word/document.xml") == expected_document
        assert b"customXml" not in archive.read("word/_rels/document.xml.rels")
        assert b"/customXml/" not in archive.read("[Content_Types].xml")


def test_bundled_accounting_reference_template_is_free_of_hidden_custom_xml() -> None:
    template = ROOT / "content-packs" / "tier1-accounting-ru" / "templates" / "service_act.docx"
    with zipfile.ZipFile(template) as archive:
        assert not any(name.lower().startswith("customxml/") for name in archive.namelist())
        document = archive.read("word/document.xml")
    assert b"{{document.number}}" in document
    assert b"{{amount.total}}" in document
