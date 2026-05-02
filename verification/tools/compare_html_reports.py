#!/usr/bin/env python3
import argparse
import difflib
import json
import re
import sys
from pathlib import Path


def extract_balanced_json(text: str, start: int):
   opening = text[start]
   closing = {"[": "]", "{": "}", "(": ")"}.get(opening)
   if closing is None:
      raise ValueError(f"unsupported JSON block opener: {opening}")
   depth = 0
   in_string = False
   quote = ""
   escape = False
   for i in range(start, len(text)):
      ch = text[i]
      if in_string:
         if escape:
            escape = False
         elif ch == "\\":
            escape = True
         elif ch == quote:
            in_string = False
         continue
      if ch in ('"', "'"):
         in_string = True
         quote = ch
      elif ch == opening:
         depth += 1
      elif ch == closing:
         depth -= 1
         if depth == 0:
            return text[start : i + 1]
   raise ValueError("unterminated JSON block")


def normalize_trace(path: Path):
   text = path.read_text(encoding="utf-8")
   match = re.search(r"var\s+tableData\s*=\s*", text)
   if not match:
      raise ValueError(f"{path}: missing tableData assignment")
   start = text.find("[", match.end())
   if start < 0:
      raise ValueError(f"{path}: missing tableData JSON array")
   data = json.loads(extract_balanced_json(text, start))
   return {"kind": "trace", "tableData": normalize_trace_table_data(data)}


def normalize_trace_table_data(data):
   for iteration in data:
      for row in iteration:
         display = row.get("str")
         if isinstance(display, str):
            row["str"] = normalize_trace_display(display)
   return data


def normalize_trace_display(display: str) -> str:
   # Python's XED path prints conditional branch aliases/targets differently
   # from Rust XED's formatter (e.g. `jnz 0xfffffffffffffffa` vs `jne 0x0`).
   # The report semantics are the same when the uops.info href identifies the
   # same branch instruction, so normalize only the anchor text for JNZ relbrs.
   if "html-instr/JNZ_Rel" not in display:
      return display
   return re.sub(r">j(?:nz|ne)\s+0x[0-9a-fA-F]+<", ">jnz <rel><", display)


def split_js_args(text: str):
   args = []
   start = 0
   stack = []
   in_string = False
   quote = ""
   escape = False
   pairs = {"[": "]", "{": "}", "(": ")"}
   for i, ch in enumerate(text):
      if in_string:
         if escape:
            escape = False
         elif ch == "\\":
            escape = True
         elif ch == quote:
            in_string = False
         continue
      if ch in ('"', "'"):
         in_string = True
         quote = ch
      elif ch in pairs:
         stack.append(pairs[ch])
      elif stack and ch == stack[-1]:
         stack.pop()
      elif ch == "," and not stack:
         args.append(text[start:i].strip())
         start = i + 1
   args.append(text[start:].strip())
   return args


def extract_plotly_args(text: str, call: int):
   open_paren = text.find("(", call)
   if open_paren < 0:
      raise ValueError("missing Plotly.newPlot argument list")
   args_text = extract_balanced_json(text, open_paren)[1:-1]
   return split_js_args(args_text)


def find_data_assignment(text: str, name: str, call: int):
   pattern = re.compile(r"\b(?:const|var|let)\s+" + re.escape(name) + r"\s*=")
   matches = list(pattern.finditer(text))
   if not matches:
      raise ValueError(f"missing Plotly series data assignment for {name}")
   before = [match for match in matches if match.start() < call]
   match = before[-1] if before else matches[0]
   start = match.end()
   while start < len(text) and text[start].isspace():
      start += 1
   if start >= len(text) or text[start] != "[":
      raise ValueError(f"Plotly series data assignment for {name} is not an array")
   return json.loads(extract_balanced_json(text, start))


def extract_graph_series(text: str, call: int):
   args = extract_plotly_args(text, call)
   if len(args) < 2:
      raise ValueError("missing Plotly series argument")
   data_arg = args[1]
   if data_arg.startswith("["):
      return json.loads(data_arg)
   if re.fullmatch(r"[$A-Za-z_][$\w]*", data_arg):
      return find_data_assignment(text, data_arg, call)
   raise ValueError(f"unsupported Plotly series argument: {data_arg}")


def normalize_graph_series(raw_series):
   if not isinstance(raw_series, list):
      raise ValueError("Plotly series data must be an array")
   normalized = []
   for index, item in enumerate(raw_series):
      if not isinstance(item, dict):
         raise ValueError(f"Plotly series item {index} must be an object")
      line = item.get("line") or {}
      if not isinstance(line, dict):
         line = {}
      normalized.append(
         {
            "name": item.get("name"),
            "y": item.get("y"),
            "mode": item.get("mode"),
            "lineShape": item.get("lineShape", line.get("shape")),
         }
      )
   return normalized


def normalize_graph(path: Path):
   text = path.read_text(encoding="utf-8")
   call = text.find("Plotly.newPlot")
   if call < 0:
      raise ValueError(f"{path}: missing Plotly.newPlot call")
   series = extract_graph_series(text, call)
   return {
      "kind": "graph",
      "series": normalize_graph_series(series),
      "controls": normalize_graph_controls(text),
   }


def normalize_graph_controls(text: str):
   has_toggle_label = "Toggle interpolation mode" in text
   has_line_shape_toggle = "line.shape" in text and "linear" in text and "hv" in text
   return {"interpolationToggle": has_toggle_label and has_line_shape_toggle}


def normalize(kind: str, path: Path):
   if kind == "trace":
      return normalize_trace(path)
   if kind == "graph":
      return normalize_graph(path)
   raise ValueError(f"unknown report kind: {kind}")


def main(argv=None):
   parser = argparse.ArgumentParser(description="Compare uiCA HTML reports semantically")
   parser.add_argument("--kind", choices=["trace", "graph"], required=True)
   parser.add_argument("left", type=Path)
   parser.add_argument("right", type=Path)
   args = parser.parse_args(argv)

   try:
      left = normalize(args.kind, args.left)
      right = normalize(args.kind, args.right)
   except (OSError, ValueError) as e:
      print(f"error: {e}", file=sys.stderr)
      return 1

   if left == right:
      print("semantic reports match")
      return 0

   left_text = json.dumps(left, indent=2, sort_keys=True).splitlines(keepends=True)
   right_text = json.dumps(right, indent=2, sort_keys=True).splitlines(keepends=True)
   sys.stdout.writelines(
      difflib.unified_diff(left_text, right_text, fromfile="left", tofile="right", n=10)
   )
   return 1


if __name__ == "__main__":
   raise SystemExit(main())
