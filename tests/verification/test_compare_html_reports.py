import json
import re
import subprocess
import sys
from pathlib import Path

TOOL = Path(__file__).resolve().parents[2] / "verification" / "tools" / "compare_html_reports.py"


def write_trace(path: Path, table_data, *, pretty=False, event_to_color=None):
   table_json = json.dumps(table_data, indent=2 if pretty else None)
   color_json = json.dumps(event_to_color or {}, indent=2 if pretty else None)
   if pretty:
      path.write_text(
         """
<!doctype html>
<html>
  <head>
    <title>trace fixture</title>
    <script>
      var reportConfig = {"ignored": true};
      var tableData = %s;
      var eventToColor = %s;
    </script>
  </head>
  <body>
    <main><table id="traceTable"></table></main>
  </body>
</html>
"""
         % (table_json, color_json),
         encoding="utf-8",
      )
      return

   path.write_text(
      """
<!DOCTYPE html>
<html><head><script>
var tableData = %s
var eventToColor = %s
</script></head><body><table id="traceTable"></table></body></html>
"""
      % (table_json, color_json),
      encoding="utf-8",
   )


def write_graph(path: Path, series, *, pretty=False, layout=None, config=None, div_id="abc"):
   series_json = json.dumps(series, indent=2 if pretty else None)
   layout_json = json.dumps(layout or {}, indent=2 if pretty else None)
   config_json = json.dumps(
      config if config is not None else {"displayModeBar": True}, indent=2 if pretty else None
   )
   if pretty:
      path.write_text(
         """
<!doctype html>
<html>
  <head><meta charset="utf-8"></head>
  <body>
    <section class="report">
      <div id="%s"></div>
      <script>
        Plotly.newPlot("%s", %s, %s, %s);
      </script>
    </section>
  </body>
</html>
"""
         % (div_id, div_id, series_json, layout_json, config_json),
         encoding="utf-8",
      )
      return

   path.write_text(
      """
<html><body><script>
Plotly.newPlot("%s", %s, %s, %s);
</script></body></html>
"""
      % (div_id, series_json, layout_json, config_json),
      encoding="utf-8",
   )


def write_graph_data_variable(path: Path, series, *, keyword="const", name="data", div_id="abc"):
   path.write_text(
      """
<html><body><script>
%s %s = %s;
const layout = {};
const config = {"displayModeBar": true};
Plotly.newPlot("%s", %s, layout, config);
</script></body></html>
"""
      % (keyword, name, json.dumps(series), div_id, name),
      encoding="utf-8",
   )


def run_tool(kind: str, left: Path, right: Path):
   return subprocess.run(
      [sys.executable, str(TOOL), "--kind", kind, str(left), str(right)],
      text=True,
      stdout=subprocess.PIPE,
      stderr=subprocess.PIPE,
      check=False,
   )


def test_trace_report_parse_errors_are_clean(tmp_path):
   bad_reports = [
      "<html><body>no trace data</body></html>",
      "<script>var tableData = [{bad json}];</script>",
   ]

   for i, bad_html in enumerate(bad_reports):
      left = tmp_path / f"bad-trace-{i}.html"
      right = tmp_path / f"right-trace-{i}.html"
      left.write_text(bad_html, encoding="utf-8")
      write_trace(right, [[{"str": "add", "uops": []}]])

      result = run_tool("trace", left, right)

      assert result.returncode != 0
      assert "error:" in result.stderr
      assert "Traceback" not in result.stderr


def test_trace_reports_match_after_html_normalization(tmp_path):
   table_data = [
      [{"str": "add rax, rbx", "uops": [{"possiblePorts": "{0}", "actualPort": "0", "events": {"1": "D"}}]}]
   ]
   left = tmp_path / "left-trace.html"
   right = tmp_path / "right-trace.html"
   write_trace(left, table_data)
   write_trace(right, table_data, pretty=True, event_to_color={"D": "#ffcc00"})

   assert left.read_text(encoding="utf-8") != right.read_text(encoding="utf-8")

   result = run_tool("trace", left, right)

   assert result.returncode == 0, result.stdout + result.stderr
   assert "semantic reports match" in result.stdout


def test_trace_reports_normalize_jnz_alias_display(tmp_path):
   left = tmp_path / "left-trace.html"
   right = tmp_path / "right-trace.html"
   write_trace(
      left,
      [[{"str": '<a href="https://www.uops.info/html-instr/JNZ_Rel8.html" target="_blank">jnz 0xfffffffffffffffa</a>', "uops": []}]],
   )
   write_trace(
      right,
      [[{"str": '<a href="https://www.uops.info/html-instr/JNZ_Rel8.html" target="_blank">jne 0x0</a>', "uops": []}]],
   )

   result = run_tool("trace", left, right)

   assert result.returncode == 0, result.stdout + result.stderr
   assert "semantic reports match" in result.stdout


def test_trace_reports_diff_event_cycle(tmp_path):
   left = tmp_path / "left-trace.html"
   right = tmp_path / "right-trace.html"
   write_trace(left, [[{"str": "add", "uops": [{"possiblePorts": "{0}", "actualPort": "0", "events": {"1": "D"}}]}]])
   write_trace(
      right, [[{"str": "add", "uops": [{"possiblePorts": "{0}", "actualPort": "0", "events": {"2": "D"}}]}]]
   )

   result = run_tool("trace", left, right)

   assert result.returncode == 1
   assert "--- left" in result.stdout
   assert re.search(r'^\+\s*"2": "D"', result.stdout, re.MULTILINE)


def test_graph_reports_match_plotly_series(tmp_path):
   left_series = [{"name": "IQ", "y": [0, 1, 1], "mode": "lines+markers", "line": {"shape": "hv"}}]
   right_series = [{"name": "IQ", "y": [0, 1, 1], "mode": "lines+markers", "lineShape": "hv"}]
   left = tmp_path / "left-graph.html"
   right = tmp_path / "right-graph.html"
   write_graph(left, left_series)
   write_graph(
      right,
      right_series,
      pretty=True,
      layout={"showlegend": True},
      config={"displayModeBar": False},
      div_id="normalized-graph",
   )

   assert left.read_text(encoding="utf-8") != right.read_text(encoding="utf-8")

   result = run_tool("graph", left, right)

   assert result.returncode == 0, result.stdout + result.stderr
   assert "semantic reports match" in result.stdout


def test_graph_reports_diff_interpolation_toggle(tmp_path):
   series = [{"name": "IQ", "y": [0, 1, 1], "mode": "lines+markers", "line": {"shape": "hv"}}]
   left = tmp_path / "left-graph.html"
   right = tmp_path / "right-graph.html"
   write_graph(left, series)
   right.write_text(
      '''
<html><body><script>
const data = [{"name": "IQ", "y": [0, 1, 1], "mode": "lines+markers", "line": {"shape": "hv"}}];
const config = {"modeBarButtonsToAdd": [{"name": "Toggle interpolation mode"}]};
function toggleInterpolation(gd) {
  const next = gd.data[0].line.shape === "hv" ? "linear" : "hv";
  Plotly.restyle(gd, "line.shape", next);
}
Plotly.newPlot("abc", data, {}, config);
</script></body></html>
''',
      encoding="utf-8",
   )

   result = run_tool("graph", left, right)

   assert result.returncode == 1
   assert "interpolationToggle" in result.stdout


def test_graph_reports_match_plotly_data_variable(tmp_path):
   series = [{"name": "IQ", "y": [0, 1, 1], "mode": "lines+markers", "line": {"shape": "hv"}}]
   left = tmp_path / "left-graph.html"
   right = tmp_path / "right-graph.html"
   write_graph(left, series)
   write_graph_data_variable(right, series)

   result = run_tool("graph", left, right)

   assert result.returncode == 0, result.stdout + result.stderr
   assert "semantic reports match" in result.stdout


def test_graph_report_malformed_series_item_error_is_clean(tmp_path):
   left = tmp_path / "bad-graph.html"
   right = tmp_path / "right-graph.html"
   left.write_text('<script>Plotly.newPlot("abc", [1], {}, {});</script>', encoding="utf-8")
   write_graph(right, [{"name": "IQ", "y": [0], "mode": "lines+markers", "line": {"shape": "hv"}}])

   result = run_tool("graph", left, right)

   assert result.returncode != 0
   assert "error:" in result.stderr
   assert "Plotly series item 0 must be an object" in result.stderr
   assert "Traceback" not in result.stderr


def test_graph_reports_diff_series_values(tmp_path):
   left = tmp_path / "left-graph.html"
   right = tmp_path / "right-graph.html"
   write_graph(left, [{"name": "IQ", "y": [0, 1], "mode": "lines+markers", "line": {"shape": "hv"}}])
   write_graph(right, [{"name": "IQ", "y": [0, 2], "mode": "lines+markers", "line": {"shape": "hv"}}])

   result = run_tool("graph", left, right)

   assert result.returncode == 1
   assert "series" in result.stdout
   assert '"y"' in result.stdout
   assert re.search(r'"y"[\s\S]*[-][^\n]*1[\s\S]*[+][^\n]*2', result.stdout)
