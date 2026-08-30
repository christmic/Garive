#!/usr/bin/env python3
"""Render the screenshot-bound Chinese Desktop manual as an internal draft PDF."""

from __future__ import annotations

import argparse
import html
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.pdfdoc import PDFString
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    BaseDocTemplate,
    Frame,
    Image,
    KeepTogether,
    PageBreak,
    PageTemplate,
    Paragraph,
    Spacer,
    Table,
    TableStyle,
)
from reportlab.platypus.tableofcontents import TableOfContents
from pypdf import PdfReader, PdfWriter
from pypdf.generic import NameObject, TextStringObject

REPOSITORY = Path(__file__).resolve().parents[2]
SOURCE = REPOSITORY / "docs/manual/desktop-user-guide.md"
SPEC = REPOSITORY / "spec/design/desktop-visual-manual-evidence.md"
DEFAULT_OUTPUT = REPOSITORY / "output/pdf/garive-macos-user-guide-draft.pdf"
DEFAULT_TAGGED_OUTPUT = REPOSITORY / "output/pdf/garive-macos-user-guide-tagged-draft.pdf"
FONT_PATH = Path("/System/Library/Fonts/STHeiti Medium.ttc")
INK = colors.HexColor("#17202A")
MUTED = colors.HexColor("#5E6A73")
ACCENT = colors.HexColor("#176B63")
PALE = colors.HexColor("#EAF5F2")
WARNING = colors.HexColor("#FFF4D8")
RULE = colors.HexColor("#D7E0E3")


class ManualDocument(BaseDocTemplate):
    def __init__(self, output: Path) -> None:
        super().__init__(
            str(output), pagesize=A4, leftMargin=19 * mm, rightMargin=19 * mm,
            topMargin=20 * mm, bottomMargin=18 * mm, title="Garive macOS 用户手册 - Draft",
            author="Garive", subject="Internal screenshot-bound Desktop manual draft",
        )
        frame = Frame(self.leftMargin, self.bottomMargin, self.width, self.height, id="body")
        self.addPageTemplates(PageTemplate(id="manual", frames=[frame], onPage=self.decorate_page))

    def beforeDocument(self) -> None:
        self.canv._doc.Catalog.Lang = PDFString("zh-CN")

    def decorate_page(self, canvas, document) -> None:
        canvas.saveState()
        canvas.setStrokeColor(RULE)
        canvas.line(document.leftMargin, 14 * mm, A4[0] - document.rightMargin, 14 * mm)
        canvas.setFillColor(MUTED)
        canvas.setFont("GariveCJK", 7.5)
        canvas.drawString(document.leftMargin, 9 * mm, "Garive macOS 用户手册 · 内部草案")
        canvas.drawRightString(A4[0] - document.rightMargin, 9 * mm, str(document.page))
        canvas.restoreState()

    def afterFlowable(self, flowable) -> None:
        if not isinstance(flowable, Paragraph) or flowable.style.name not in {"H1", "H2", "H3"}:
            return
        level = {"H1": 0, "H2": 0, "H3": 1}[flowable.style.name]
        title = flowable.getPlainText()
        bookmark = getattr(flowable, "bookmark_name", f"section-{self.seq.nextf('section')}")
        self.canv.bookmarkPage(bookmark)
        self.canv.addOutlineEntry(title, bookmark, level=level, closed=level > 0)
        self.notify("TOCEntry", (level, title, self.page, bookmark))


def normalize(text: str) -> str:
    return text.replace("–", "-").replace("—", "-").replace("‑", "-")


def inline(text: str) -> str:
    escaped = html.escape(normalize(text), quote=False)
    escaped = re.sub(r"`([^`]+)`", r'<font name="Courier" color="#176B63">\1</font>', escaped)
    escaped = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", escaped)
    escaped = re.sub(
        r"\[([^]]+)]\(([^)]+)\)",
        r'<a href="\2" color="#176B63"><u>\1</u></a>', escaped,
    )
    return escaped


def semantic_inline(text: str) -> str:
    escaped = html.escape(text, quote=True)
    escaped = re.sub(r"`([^`]+)`", r"<code>\1</code>", escaped)
    escaped = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", escaped)
    return re.sub(r"\[([^]]+)]\(([^)]+)\)", r'<a href="\2">\1</a>', escaped)


def styles() -> dict[str, ParagraphStyle]:
    base = getSampleStyleSheet()
    return {
        "Title": ParagraphStyle("Title", fontName="GariveCJK", fontSize=28, leading=36,
            textColor=INK, alignment=TA_LEFT, spaceAfter=8 * mm),
        "Subtitle": ParagraphStyle("Subtitle", fontName="GariveCJK", fontSize=11, leading=17,
            textColor=MUTED, spaceAfter=6 * mm),
        "H1": ParagraphStyle("H1", fontName="GariveCJK", fontSize=19, leading=25,
            textColor=INK, spaceBefore=6 * mm, spaceAfter=3 * mm, keepWithNext=True),
        "H2": ParagraphStyle("H2", fontName="GariveCJK", fontSize=15, leading=21,
            textColor=ACCENT, spaceBefore=5 * mm, spaceAfter=2 * mm, keepWithNext=True),
        "H3": ParagraphStyle("H3", fontName="GariveCJK", fontSize=12, leading=18,
            textColor=INK, spaceBefore=4 * mm, spaceAfter=1.5 * mm, keepWithNext=True),
        "Body": ParagraphStyle("Body", fontName="GariveCJK", fontSize=9.4, leading=15,
            textColor=INK, spaceAfter=2.2 * mm, splitLongWords=False),
        "List": ParagraphStyle("List", fontName="GariveCJK", fontSize=9.2, leading=14.5,
            textColor=INK, leftIndent=6 * mm, firstLineIndent=-4 * mm, spaceAfter=1.2 * mm),
        "Quote": ParagraphStyle("Quote", fontName="GariveCJK", fontSize=9.2, leading=14.5,
            textColor=MUTED, leftIndent=5 * mm, borderColor=ACCENT, borderWidth=1,
            borderPadding=(2 * mm, 3 * mm, 2 * mm, 3 * mm), backColor=PALE, spaceAfter=3 * mm),
        "Small": ParagraphStyle("Small", fontName="GariveCJK", fontSize=8, leading=12,
            textColor=MUTED),
        "TOCHeading": ParagraphStyle("TOCHeading", fontName="GariveCJK", fontSize=18,
            leading=24, textColor=INK, spaceAfter=5 * mm),
    }


def heading(text: str, level: int, sheet: dict[str, ParagraphStyle], index: int) -> Paragraph:
    paragraph = Paragraph(inline(text), sheet[f"H{level}"])
    paragraph.bookmark_name = f"section-{index}"
    return paragraph


def markdown_table(rows: list[str], sheet: dict[str, ParagraphStyle]) -> Table:
    parsed = [[cell.strip() for cell in row.strip().strip("|").split("|")] for row in rows]
    parsed = [row for row in parsed if not all(re.fullmatch(r":?-{3,}:?", cell) for cell in row)]
    cells = [[Paragraph(inline(cell), sheet["Small"]) for cell in row] for row in parsed]
    widths = [45 * mm, 112 * mm] if len(cells[0]) == 2 else None
    table = Table(cells, colWidths=widths, repeatRows=1, hAlign="LEFT")
    table.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, 0), PALE), ("TEXTCOLOR", (0, 0), (-1, 0), INK),
        ("GRID", (0, 0), (-1, -1), 0.35, RULE), ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 5), ("RIGHTPADDING", (0, 0), (-1, -1), 5),
        ("TOPPADDING", (0, 0), (-1, -1), 4), ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
    ]))
    return table


def screenshot_placeholder(line: str, sheet: dict[str, ParagraphStyle]):
    match = re.search(r"SCREENSHOT (M\d{2}) PENDING: (.+?) -->", line)
    if not match:
        return None
    capture_id, description = match.groups()
    content = Paragraph(
        f'<b>{capture_id}</b> · 截图待录入<br/><font color="#5E6A73">{inline(description)}</font>',
        sheet["Small"],
    )
    box = Table([[content]], colWidths=[157 * mm])
    box.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, -1), WARNING), ("BOX", (0, 0), (-1, -1), 0.5, colors.HexColor("#D9B75B")),
        ("LEFTPADDING", (0, 0), (-1, -1), 7), ("RIGHTPADDING", (0, 0), (-1, -1), 7),
        ("TOPPADDING", (0, 0), (-1, -1), 6), ("BOTTOMPADDING", (0, 0), (-1, -1), 6),
    ]))
    return KeepTogether([box, Spacer(1, 2.5 * mm)])


def validate_manual(lines: list[str]) -> None:
    expected_ids = re.findall(r"\| `(M\d{2})` \|", SPEC.read_text(encoding="utf-8"))
    pending_ids = [match.group(1) for line in lines
        if (match := re.search(r"SCREENSHOT (M\d{2}) PENDING:", line))]
    image_ids = [match.group(1) for line in lines
        if (match := re.search(r"!\[(M\d{2})[^]]*]\(assets/desktop/[^)]+\.png\)", line))]
    manual_ids = pending_ids + image_ids
    if len(expected_ids) != len(set(expected_ids)):
        raise RuntimeError("accepted Desktop evidence spec contains duplicate capture IDs")
    if len(manual_ids) != len(set(manual_ids)) or set(manual_ids) != set(expected_ids):
        raise RuntimeError("manual screenshot placeholders do not match the accepted evidence spec")


def body_story(lines: list[str], sheet: dict[str, ParagraphStyle]):
    story = []
    paragraph_lines: list[str] = []
    heading_index = 0

    def flush() -> None:
        if paragraph_lines:
            story.append(Paragraph(inline(" ".join(paragraph_lines)), sheet["Body"]))
            paragraph_lines.clear()

    index = 0
    while index < len(lines):
        line = lines[index].rstrip()
        if line.startswith("|"):
            flush()
            table_rows = []
            while index < len(lines) and lines[index].rstrip().startswith("|"):
                table_rows.append(lines[index].rstrip())
                index += 1
            story.extend([markdown_table(table_rows, sheet), Spacer(1, 3 * mm)])
            continue
        if match := re.match(r"!\[(M\d{2})[^]]*]\((assets/desktop/[^)]+\.png)\)", line):
            flush()
            capture_id, relative = match.groups()
            artwork = Image(str(SOURCE.parent / relative))
            artwork._restrictSize(157 * mm, 178 * mm)
            story.extend([artwork, Paragraph(capture_id, sheet["Small"]), Spacer(1, 3 * mm)])
        elif line.startswith("<!-- SCREENSHOT"):
            flush()
            placeholder = screenshot_placeholder(line, sheet)
            if placeholder:
                story.append(placeholder)
        elif line.startswith("### "):
            flush(); heading_index += 1
            story.append(heading(line[4:], 3, sheet, heading_index))
        elif line.startswith("## "):
            flush(); heading_index += 1
            story.append(heading(line[3:], 2, sheet, heading_index))
        elif line.startswith("> "):
            flush(); story.append(Paragraph(inline(line[2:]), sheet["Quote"]))
        elif re.match(r"^\d+\. ", line):
            flush(); number, content = line.split(". ", 1)
            story.append(Paragraph(f"{number}. {inline(content)}", sheet["List"]))
        elif line.startswith("- "):
            flush(); story.append(Paragraph(f"• {inline(line[2:])}", sheet["List"]))
        elif not line:
            flush()
        elif not line.startswith("<!--"):
            paragraph_lines.append(line)
        index += 1
    flush()
    return story


def build(output: Path) -> None:
    if not FONT_PATH.exists():
        raise RuntimeError(f"required macOS CJK font is unavailable: {FONT_PATH}")
    lines = SOURCE.read_text(encoding="utf-8").splitlines()
    validate_manual(lines)
    pdfmetrics.registerFont(TTFont("GariveCJK", str(FONT_PATH), subfontIndex=0))
    sheet = styles()
    first_section = next(index for index, line in enumerate(lines) if line.startswith("## 1."))
    story = [Spacer(1, 18 * mm), Paragraph("Garive macOS 用户手册", sheet["Title"]),
        Paragraph("截图绑定的内部排版草案 · 不可作为公开发布说明", sheet["Subtitle"]),
        Paragraph("本 PDF 用于验证版式、文本提取、目录和阅读顺序。所有黄色 Mxx 方框必须由候选包真实截图替换。", sheet["Quote"]),
        Spacer(1, 4 * mm)]
    cover_tables = [line for line in lines[:first_section] if line.startswith("|")]
    story.extend([markdown_table(cover_tables, sheet), PageBreak(),
        Paragraph("目录", sheet["TOCHeading"])])
    toc = TableOfContents()
    toc.levelStyles = [ParagraphStyle(f"TOC{level}", fontName="GariveCJK", fontSize=9.5,
        leading=15, leftIndent=level * 6 * mm, textColor=INK) for level in range(3)]
    story.extend([toc, PageBreak()])
    story.extend(body_story(lines[first_section:], sheet))
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.NamedTemporaryFile(
        dir=output.parent, prefix=f".{output.name}.", suffix=".tmp", delete=False,
    ).name)
    try:
        ManualDocument(temporary).multiBuild(story)
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)


def semantic_html(lines: list[str]) -> str:
    content: list[str] = []
    index = 0
    while index < len(lines):
        line = lines[index].rstrip()
        heading_match = re.match(r"^(#{1,3}) (.+)$", line)
        if heading_match:
            level = len(heading_match.group(1))
            content.append(f"<h{level}>{semantic_inline(heading_match.group(2))}</h{level}>")
        elif line.startswith("> "):
            quote = []
            while index < len(lines) and lines[index].startswith("> "):
                quote.append(semantic_inline(lines[index][2:]))
                index += 1
            content.append(f"<blockquote>{'<br>'.join(quote)}</blockquote>")
            continue
        elif line.startswith("|"):
            rows = []
            while index < len(lines) and lines[index].startswith("|"):
                rows.append([cell.strip() for cell in lines[index].strip().strip("|").split("|")])
                index += 1
            rows = [row for row in rows if not all(re.fullmatch(r":?-{3,}:?", cell) for cell in row)]
            head = "".join(f"<th>{semantic_inline(cell)}</th>" for cell in rows[0])
            body = "".join("<tr>" + "".join(
                f"<td>{semantic_inline(cell)}</td>" for cell in row) + "</tr>" for row in rows[1:])
            content.append(f"<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>")
            continue
        elif re.match(r"^\d+\. ", line):
            items = []
            while index < len(lines) and re.match(r"^\d+\. ", lines[index]):
                number, item = lines[index].split(". ", 1)
                items.append((number, item))
                index += 1
            content.extend(
                f'<p class="ordered">{number}. {semantic_inline(item)}</p>'
                for number, item in items
            )
            continue
        elif line.startswith("- "):
            items = []
            while index < len(lines) and lines[index].startswith("- "):
                items.append(lines[index][2:])
                index += 1
            content.append("<ul>" + "".join(f"<li>{semantic_inline(item)}</li>" for item in items) + "</ul>")
            continue
        elif line.startswith("<!-- SCREENSHOT"):
            match = re.search(r"SCREENSHOT (M\d{2}) PENDING: (.+?) -->", line)
            if match:
                capture_id, description = match.groups()
                content.append(f'<p class="shot"><strong>{capture_id} · 截图待录入</strong> — '
                    f"{semantic_inline(description)}</p>")
        elif match := re.match(r"!\[((M\d{2})[^]]*)]\((assets/desktop/[^)]+\.png)\)", line):
            alt_text, capture_id, relative = match.groups()
            source = (SOURCE.parent / relative).resolve().as_uri()
            content.append(f'<figure><img src="{source}" alt="{html.escape(alt_text, quote=True)}" '
                f'style="max-width:100%"><figcaption>{capture_id}</figcaption></figure>')
        elif line:
            paragraph = [line]
            while index + 1 < len(lines) and lines[index + 1].strip() and not re.match(
                r"^(#{1,3} |>|\||\d+\. |- |<!-- SCREENSHOT|!\[)", lines[index + 1],
            ):
                index += 1
                paragraph.append(lines[index].strip())
            content.append(f"<p>{semantic_inline(' '.join(paragraph))}</p>")
        index += 1
    css = """
      @page { size: A4; margin: 18mm 19mm 18mm 19mm; }
      body { font-family: 'Heiti SC', 'Hiragino Sans GB', sans-serif; color: #17202a;
        font-size: 10pt; line-height: 1.55; }
      h1 { font-size: 25pt; margin: 16mm 0 7mm; }
      h2 { color: #176b63; font-size: 18pt; margin: 8mm 0 3mm; page-break-after: avoid; }
      h3 { font-size: 13pt; margin: 5mm 0 2mm; page-break-after: avoid; }
      p, li { margin: 0 0 2mm; } code { color: #176b63; }
      .ordered { margin-left: 7mm; }
      blockquote { background: #eaf5f2; border: 1pt solid #176b63; padding: 4mm; margin: 4mm 0; }
      table { border-collapse: collapse; width: 100%; margin: 4mm 0; }
      th { background: #eaf5f2; } th, td { border: .5pt solid #d7e0e3; padding: 2mm; text-align: left; }
      .shot { background: #fff4d8; border: .5pt solid #d9b75b; color: #5e6a73;
        padding: 3mm; margin: 2mm 5mm; page-break-inside: avoid; }
    """
    return ("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">"
        "<title>Garive macOS 用户手册 - Tagged Draft</title><style>" + css
        + "</style></head><body>" + "\n".join(content) + "</body></html>")


def build_tagged(output: Path, soffice: str | None) -> None:
    lines = SOURCE.read_text(encoding="utf-8").splitlines()
    validate_manual(lines)
    executable = soffice or os.environ.get("SOFFICE") or shutil.which("soffice")
    if not executable:
        raise RuntimeError("tagged PDF generation requires soffice on PATH or --soffice")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="garive-manual-") as directory:
        temporary = Path(directory)
        source = temporary / "garive-manual.html"
        source.write_text(semantic_html(lines), encoding="utf-8")
        profile = temporary / "profile"
        font_cache = temporary / "font-cache"
        font_config = temporary / "fonts.conf"
        font_config.write_text("""<?xml version="1.0"?>
<fontconfig>
  <dir>/System/Library/Fonts</dir><dir>/Library/Fonts</dir>
  <cachedir>{cache}</cachedir>
</fontconfig>
""".format(cache=html.escape(str(font_cache))), encoding="utf-8")
        filter_options = ('pdf:writer_pdf_Export:{"PDFUACompliance":{"type":"boolean","value":"true"},'
            '"UseTaggedPDF":{"type":"boolean","value":"true"},'
            '"ExportBookmarks":{"type":"boolean","value":"true"}}')
        environment = {**os.environ, "FONTCONFIG_FILE": str(font_config)}
        subprocess.run([executable, f"-env:UserInstallation={profile.as_uri()}", "--headless",
            "--convert-to", filter_options, "--outdir", str(temporary), str(source)],
            check=True, env=environment)
        generated = temporary / "garive-manual.pdf"
        if not generated.is_file():
            raise RuntimeError("soffice did not produce the expected tagged PDF")
        normalized = temporary / "garive-manual-zh-CN.pdf"
        reader = PdfReader(str(generated))
        writer = PdfWriter(clone_from=reader)
        writer.pdf_header = reader.pdf_header
        writer.root_object[NameObject("/Lang")] = TextStringObject("zh-CN")
        with normalized.open("wb") as stream:
            writer.write(stream)
        normalized.replace(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", nargs="?", type=Path)
    parser.add_argument("--tagged", action="store_true")
    parser.add_argument("--soffice")
    arguments = parser.parse_args()
    destination = (arguments.output or (DEFAULT_TAGGED_OUTPUT if arguments.tagged else DEFAULT_OUTPUT)).resolve()
    build_tagged(destination, arguments.soffice) if arguments.tagged else build(destination)
