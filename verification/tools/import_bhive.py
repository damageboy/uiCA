import argparse
import csv
import hashlib
import sys
import urllib.request
from dataclasses import dataclass
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from verification.tools.common import verification_root

BHIVE_THROUGHPUT_URLS = {
   "HSW": "https://raw.githubusercontent.com/ithemal/bhive/master/benchmark/throughput/hsw.csv",
   "IVB": "https://raw.githubusercontent.com/ithemal/bhive/master/benchmark/throughput/ivb.csv",
   "SKL": "https://raw.githubusercontent.com/ithemal/bhive/master/benchmark/throughput/skl.csv",
}

DEFAULT_RUN_CONFIG = {
   "alignmentOffset": 0,
   "initPolicy": "diff",
   "noMicroFusion": False,
   "noMacroFusion": False,
   "simpleFrontEnd": False,
   "minIterations": 10,
   "minCycles": 500,
}


@dataclass(frozen=True)
class BhiveRow:
   hex_text: str
   measured_cycles_per_100_iterations: float

   @property
   def measured_cycles_per_iteration(self) -> float:
      return self.measured_cycles_per_100_iterations / 100.0


@dataclass(frozen=True)
class ImportResult:
   profile_path: Path
   case_paths: list[Path]
   case_ids: list[str]


def default_source_for_arch(arch: str) -> str:
   arch = arch.upper()
   if arch not in BHIVE_THROUGHPUT_URLS:
      choices = ", ".join(sorted(BHIVE_THROUGHPUT_URLS))
      raise ValueError(f"unsupported BHive arch {arch}; choose one of {choices}")
   return BHIVE_THROUGHPUT_URLS[arch]


def _open_source(source: str):
   if source.startswith(("http://", "https://")):
      return urllib.request.urlopen(source, timeout=60)
   return open(source, "rb")


def load_bhive_rows(source: str) -> list[BhiveRow]:
   rows: list[BhiveRow] = []
   with _open_source(source) as f:
      text_iter = (line.decode("utf-8") for line in f)
      for raw_hex, raw_measured, *_rest in csv.reader(text_iter):
         hex_text = raw_hex.strip().lower()
         if not _is_valid_hex(hex_text):
            continue
         try:
            measured = float(raw_measured)
         except ValueError:
            continue
         rows.append(BhiveRow(hex_text=hex_text, measured_cycles_per_100_iterations=measured))
   return rows


def _is_valid_hex(hex_text: str) -> bool:
   if not hex_text or len(hex_text) % 2:
      return False
   try:
      bytes.fromhex(hex_text)
   except ValueError:
      return False
   return True


def sample_rows(rows: list[BhiveRow], limit: int) -> list[BhiveRow]:
   if limit < 1:
      raise ValueError("--limit must be >= 1")
   if len(rows) <= limit:
      return rows
   if limit == 1:
      return [rows[0]]

   last = len(rows) - 1
   indexes = [round(i * last / (limit - 1)) for i in range(limit)]
   return [rows[i] for i in indexes]


def case_id_for_row(arch: str, index: int, row: BhiveRow) -> str:
   digest = hashlib.sha256(row.hex_text.encode("ascii")).hexdigest()[:8]
   return f"bhive/{arch.lower()}_{index:06d}_{digest}"


def _toml_string(value: str) -> str:
   return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def _write_case_manifest(path: Path, case_id: str, arch: str, row: BhiveRow, source: str) -> None:
   path.parent.mkdir(parents=True, exist_ok=True)
   measured_100 = row.measured_cycles_per_100_iterations
   measured_1 = row.measured_cycles_per_iteration
   path.write_text(
      "\n".join(
         [
            f"id = {_toml_string(case_id)}",
            f"description = {_toml_string(f'BHive {arch} raw basic block sample')}",
            f"tags = [\"bhive\", \"raw\", {_toml_string(arch.lower())}]",
            "",
            "[input]",
            "format = \"hex\"",
            f"hex = {_toml_string(row.hex_text)}",
            f"source = {_toml_string(source)}",
            f"measuredCyclesPer100Iterations = {measured_100!r}",
            f"measuredCyclesPerIteration = {measured_1!r}",
            "",
            "[run]",
            f"arches = [{_toml_string(arch)}]",
            f"alignmentOffset = {DEFAULT_RUN_CONFIG['alignmentOffset']}",
            f"initPolicy = {_toml_string(DEFAULT_RUN_CONFIG['initPolicy'])}",
            f"noMicroFusion = {str(DEFAULT_RUN_CONFIG['noMicroFusion']).lower()}",
            f"noMacroFusion = {str(DEFAULT_RUN_CONFIG['noMacroFusion']).lower()}",
            f"simpleFrontEnd = {str(DEFAULT_RUN_CONFIG['simpleFrontEnd']).lower()}",
            f"minIterations = {DEFAULT_RUN_CONFIG['minIterations']}",
            f"minCycles = {DEFAULT_RUN_CONFIG['minCycles']}",
            "",
         ]
      )
   )


def _write_profile(path: Path, profile: str, arch: str, case_ids: list[str]) -> None:
   path.parent.mkdir(parents=True, exist_ok=True)
   case_lines = [f"  {_toml_string(case_id)}," for case_id in case_ids]
   path.write_text(
      "\n".join(
         [
            f"name = {_toml_string(profile)}",
            "cases = [",
            *case_lines,
            "]",
            f"arches = [{_toml_string(arch)}]",
            "",
         ]
      )
   )


def import_bhive(
   *,
   arch: str,
   source: str,
   limit: int,
   profile: str,
   cases_root: Path,
   profiles_root: Path,
) -> ImportResult:
   arch = arch.upper()
   rows = sample_rows(load_bhive_rows(source), limit)
   if not rows:
      raise ValueError(f"no valid BHive rows found in {source}")

   case_paths: list[Path] = []
   case_ids: list[str] = []
   for index, row in enumerate(rows):
      case_id = case_id_for_row(arch, index, row)
      case_name = case_id.split("/", 1)[1]
      case_path = cases_root / f"{case_name}.toml"
      _write_case_manifest(case_path, case_id, arch, row, source)
      case_paths.append(case_path)
      case_ids.append(case_id)

   profile_path = profiles_root / f"{profile}.toml"
   _write_profile(profile_path, profile, arch, case_ids)
   return ImportResult(profile_path=profile_path, case_paths=case_paths, case_ids=case_ids)


def build_parser() -> argparse.ArgumentParser:
   parser = argparse.ArgumentParser(
      description="Import sampled BHive raw-hex basic blocks as verification corpus cases.",
      formatter_class=argparse.ArgumentDefaultsHelpFormatter,
   )
   parser.add_argument("--arch", required=True, choices=sorted(BHIVE_THROUGHPUT_URLS))
   parser.add_argument("--source", help="CSV path/URL; defaults to upstream BHive throughput CSV for arch")
   parser.add_argument("--limit", type=int, default=50, help="number of rows to sample")
   parser.add_argument("--profile", default="bhive_smoke", help="profile name to write")
   parser.add_argument(
      "--cases-root",
      type=Path,
      default=verification_root() / "corpora" / "bhive" / "cases",
      help="output directory for corpus case manifests",
   )
   parser.add_argument(
      "--profiles-root",
      type=Path,
      default=verification_root() / "profiles",
      help="output directory for generated profile",
   )
   return parser


def main(argv=None) -> int:
   parser = build_parser()
   try:
      args = parser.parse_args(argv)
      source = args.source or default_source_for_arch(args.arch)
      result = import_bhive(
         arch=args.arch,
         source=source,
         limit=args.limit,
         profile=args.profile,
         cases_root=args.cases_root,
         profiles_root=args.profiles_root,
      )
      print(
         f"Imported {len(result.case_ids)} BHive {args.arch} cases into "
         f"{result.profile_path.as_posix()}"
      )
      return 0
   except (OSError, ValueError) as exc:
      print(f"import failed: {exc}", file=sys.stderr)
      return 1


if __name__ == "__main__":
   raise SystemExit(main())
