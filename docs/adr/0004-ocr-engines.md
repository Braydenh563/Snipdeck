# ADR-0004: OS-native OCR, optional RapidOCR pack

- Status: Accepted (2026-10-17)

## Decision
Default OCR = `Windows.Media.Ocr` (macOS: Vision). Optional **RapidOCR** backend (ONNX via `ort`) downloaded on demand as a signed, hash-verified pack. Behind an `OcrEngine` trait.

## Consequences
Zero installer cost by default; better accuracy available for those who want it; no ML shipped unless chosen.
