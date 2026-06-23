//! Command-line tools for parsing, writing, diffing, and verifying ADO corpus files.
//!
//! The default CLI is portable Rust; the optional `oracle` feature adds
//! Windows/COM commands used to compare native output with MDAC.

use std::path::{Path, PathBuf};
#[cfg(feature = "oracle")]
use std::process::Command as ProcessCommand;
#[cfg(feature = "oracle")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "oracle")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "oracle")]
use std::thread;
#[cfg(feature = "oracle")]
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "oracle")]
use serde::Deserialize;
use tablegram::adtg::{inspect_adtg, parse_adtg_bytes};
#[cfg(feature = "oracle")]
use tablegram::compat::{materialize_affected_view, materialize_conflicting_view};
use tablegram::compat::{
    materialize_default_view, materialize_pending_view, MaterializedRecordset,
};
#[cfg(feature = "oracle")]
use tablegram::corpus_policy::is_documented_com_verification_skip_path;
use tablegram::corpus_policy::is_documented_native_adtg_only_path;
#[cfg(feature = "oracle")]
use tablegram::detect::strip_utf8_bom;
use tablegram::detect::{detect_format, RecordsetFormat};
use tablegram::hexdiff::{hexdiff, HexDiffOptions};
#[cfg(feature = "oracle")]
use tablegram::model::{
    AdoDataType, Field, FieldAttribute, Row, RowChange, RowChangeKind, RowState,
};
use tablegram::model::{RecordStatusFlag, Recordset, Value};
use tablegram::native_compare::{compare_mdac_resaved_recordsets, compare_native_recordsets};
use tablegram::xml::parse_ado_xml_bytes;
use tablegram::{parse_recordset_bytes, parse_recordset_file, write_ado_xml, write_adtg};

#[cfg(feature = "oracle")]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Parse {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = InputFormat::Auto)]
        format: InputFormat,
        #[arg(long)]
        json: bool,
    },
    InspectAdtg {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    #[cfg(feature = "oracle")]
    ParseAdtgCom {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        cscript: Option<PathBuf>,
    },
    ParseAdtgNative {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    #[cfg(feature = "oracle")]
    VerifyCom {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        cscript: Option<PathBuf>,
    },
    #[cfg(feature = "oracle")]
    VerifyWriterCom {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(long)]
        cscript: Option<PathBuf>,
        #[arg(long)]
        keep_artifacts: bool,
    },
    #[cfg(feature = "oracle")]
    VerifyCorpus {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(long)]
        cscript: Option<PathBuf>,
        #[arg(long, default_value_t = default_job_count())]
        jobs: usize,
        #[arg(long, value_enum, default_value_t = VerifyFormat::All)]
        format: VerifyFormat,
    },
    VerifyNativeCorpus {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(long)]
        mdac_resave_normalization: bool,
    },
    CompareNative {
        #[arg(long)]
        left: PathBuf,
        #[arg(long)]
        right: PathBuf,
        #[arg(long)]
        mdac_resave_normalization: bool,
    },
    WriteXml {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    WriteAdtg {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    Hexdiff {
        left: PathBuf,
        right: PathBuf,
        #[arg(long, default_value_t = 16)]
        width: usize,
        #[arg(long, default_value_t = 0)]
        max_lines: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InputFormat {
    Auto,
    Xml,
    Adtg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[cfg(feature = "oracle")]
enum VerifyFormat {
    All,
    Xml,
    Adtg,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse {
            input,
            format,
            json,
        } => parse_command(input, format, json),
        Command::InspectAdtg { input, json } => inspect_adtg_command(input, json),
        #[cfg(feature = "oracle")]
        Command::ParseAdtgCom {
            input,
            json,
            cscript,
        } => parse_adtg_com_command(input, json, cscript),
        Command::ParseAdtgNative { input, json } => parse_adtg_native_command(input, json),
        #[cfg(feature = "oracle")]
        Command::VerifyCom { input, cscript } => verify_com_command(input, cscript),
        #[cfg(feature = "oracle")]
        Command::VerifyWriterCom {
            input,
            cscript,
            keep_artifacts,
        } => verify_writer_com_command(input, cscript, keep_artifacts),
        #[cfg(feature = "oracle")]
        Command::VerifyCorpus {
            dir,
            cscript,
            jobs,
            format,
        } => verify_corpus_command(dir, cscript, jobs, format),
        Command::VerifyNativeCorpus {
            dir,
            mdac_resave_normalization,
        } => verify_native_corpus_command(dir, mdac_resave_normalization),
        Command::CompareNative {
            left,
            right,
            mdac_resave_normalization,
        } => compare_native_command(left, right, mdac_resave_normalization),
        Command::WriteXml { input, output } => write_xml_command(input, output),
        Command::WriteAdtg { input, output } => write_adtg_command(input, output),
        Command::Hexdiff {
            left,
            right,
            width,
            max_lines,
        } => hexdiff_command(left, right, width, max_lines),
    }
}

fn parse_command(input: PathBuf, format: InputFormat, json: bool) -> Result<()> {
    let bytes = std::fs::read(&input).with_context(|| format!("failed to read {input:?}"))?;
    let actual = match format {
        InputFormat::Auto => detect_format(&bytes),
        InputFormat::Xml => RecordsetFormat::Xml,
        InputFormat::Adtg => RecordsetFormat::Adtg,
    };

    match actual {
        RecordsetFormat::Xml => {
            let recordset = match format {
                InputFormat::Auto => parse_recordset_bytes(&bytes),
                InputFormat::Xml | InputFormat::Adtg => parse_ado_xml_bytes(&bytes),
            }?;
            if json {
                println!("{}", serde_json::to_string_pretty(&recordset)?);
            } else {
                println!(
                    "ADO XML: {} fields, {} rows",
                    recordset.fields.len(),
                    recordset.rows.len()
                );
                for (index, field) in recordset.fields.iter().enumerate() {
                    println!(
                        "  {:>3}: {} xml={} type={} nullable={}",
                        index + 1,
                        field.name,
                        field.xml_name,
                        field.data_type.as_deref().unwrap_or("string"),
                        field.nullable
                    );
                }
            }
        }
        RecordsetFormat::Adtg => {
            let recordset = match format {
                InputFormat::Auto => parse_recordset_bytes(&bytes),
                InputFormat::Xml | InputFormat::Adtg => parse_adtg_bytes(&bytes),
            }
            .with_context(|| format!("native ADTG parse failed for {}", input.display()))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&recordset)?);
            } else {
                print_native_adtg_recordset_summary(&recordset);
            }
        }
    }

    Ok(())
}

fn write_xml_command(input: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let recordset = parse_recordset_file(&input)?;
    let xml = write_ado_xml(&recordset)
        .with_context(|| format!("failed to write ADO XML for {}", input.display()))?;
    if let Some(output) = output {
        std::fs::write(&output, xml).with_context(|| format!("failed to write {output:?}"))?;
    } else {
        use std::io::Write;
        std::io::stdout()
            .write_all(&xml)
            .context("failed to write ADO XML to stdout")?;
    }
    Ok(())
}

fn write_adtg_command(input: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let recordset = parse_recordset_file(&input)?;
    let adtg = write_adtg(&recordset)
        .with_context(|| format!("failed to write ADTG for {}", input.display()))?;
    if let Some(output) = output {
        std::fs::write(&output, adtg).with_context(|| format!("failed to write {output:?}"))?;
    } else {
        use std::io::Write;
        std::io::stdout()
            .write_all(&adtg)
            .context("failed to write ADTG to stdout")?;
    }
    Ok(())
}

fn inspect_adtg_command(input: PathBuf, json: bool) -> Result<()> {
    let bytes = std::fs::read(&input).with_context(|| format!("failed to read {input:?}"))?;
    if detect_format(&bytes) != RecordsetFormat::Adtg {
        bail!("input looks like XML; use parse --format xml");
    }

    let document = inspect_adtg(&bytes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&document)?);
    } else {
        print_adtg_summary(&document);
    }

    Ok(())
}

#[cfg(feature = "oracle")]
fn parse_adtg_com_command(input: PathBuf, json: bool, cscript: Option<PathBuf>) -> Result<()> {
    let recordset = parse_adtg_via_com(&input, cscript.as_deref())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&recordset)?);
    } else {
        println!(
            "ADO ADTG via MSPersist: {} fields, {} rows, {} changes",
            recordset.fields.len(),
            recordset.rows.len(),
            recordset.changes.len()
        );
        for (index, field) in recordset.fields.iter().enumerate() {
            println!(
                "  {:>3}: {} xml={} type={} ado={} nullable={} writable={}",
                index + 1,
                field.name,
                field.xml_name,
                field.data_type.as_deref().unwrap_or("string"),
                field.ado_type.map(|ty| ty.name).unwrap_or("unknown"),
                field.nullable,
                field.writable
            );
        }
    }

    Ok(())
}

fn parse_adtg_native_command(input: PathBuf, json: bool) -> Result<()> {
    let bytes = std::fs::read(&input).with_context(|| format!("failed to read {input:?}"))?;
    let recordset = parse_adtg_bytes(&bytes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&recordset)?);
    } else {
        print_native_adtg_recordset_summary(&recordset);
    }
    Ok(())
}

fn print_native_adtg_recordset_summary(recordset: &Recordset) {
    println!(
        "ADO ADTG native: {} fields, {} rows, {} changes",
        recordset.fields.len(),
        recordset.rows.len(),
        recordset.changes.len()
    );
    for (index, field) in recordset.fields.iter().enumerate() {
        println!(
            "  {:>3}: {} type={} ado={} nullable={} writable={}",
            index + 1,
            field.name,
            field.data_type.as_deref().unwrap_or("unknown"),
            field.ado_type.map(|ty| ty.name).unwrap_or("unknown"),
            field.nullable,
            field.writable
        );
    }
}

fn verify_native_corpus_command(dir: PathBuf, mdac_resave_normalization: bool) -> Result<()> {
    let mut paths = corpus_recordset_paths(&dir)?;
    paths.sort();

    let mut xml_count = 0usize;
    let mut adtg_count = 0usize;
    let mut pair_count = 0usize;
    let mut adtg_only_count = 0usize;
    let mut failures = Vec::new();

    for path in &paths {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("xml") => {
                xml_count += 1;
                if let Err(err) = parse_recordset_file(path) {
                    failures.push(format!("{}: {err:#}", path.display()));
                }
            }
            Some("adtg") => {
                adtg_count += 1;
                let adtg = match parse_recordset_file(path) {
                    Ok(adtg) => adtg,
                    Err(err) => {
                        failures.push(format!("{}: {err:#}", path.display()));
                        continue;
                    }
                };

                if is_documented_native_adtg_only_path(path) {
                    match verify_native_adtg_only_artifact(path, &adtg) {
                        Ok(()) => adtg_only_count += 1,
                        Err(err) => failures.push(format!("{}: {err:#}", path.display())),
                    }
                    continue;
                }

                let Some(xml_path) = native_comparison_xml_path(path) else {
                    failures.push(format!(
                        "{}: missing matching .roundtrip.xml or same-stem .xml for native ADTG/XML comparison",
                        path.display()
                    ));
                    continue;
                };

                match parse_recordset_file(&xml_path) {
                    Ok(xml) => {
                        let use_mdac_resave_normalization =
                            mdac_resave_normalization || is_roundtrip_xml_path(&xml_path);
                        let mismatches = if use_mdac_resave_normalization {
                            compare_mdac_resaved_recordsets(&xml, &adtg)
                        } else {
                            compare_native_recordsets(&xml, &adtg)
                        };
                        if mismatches.is_empty() {
                            pair_count += 1;
                        } else {
                            failures.push(format!(
                                "{} vs {}:\n{}",
                                path.display(),
                                xml_path.display(),
                                mismatches.join("\n")
                            ));
                        }
                    }
                    Err(err) => {
                        failures.push(format!("{}: {err:#}", xml_path.display()));
                    }
                }
            }
            _ => {}
        }
    }

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("{failure}");
        }
        bail!(
            "native corpus verification failed: {} XML parsed, {} ADTG parsed, {} pairs checked, {} ADTG-only artifacts checked, {} failures",
            xml_count,
            adtg_count,
            pair_count,
            adtg_only_count,
            failures.len()
        );
    }

    if adtg_only_count == 0 {
        println!(
            "Native corpus verification ok: {xml_count} XML files, {adtg_count} ADTG files, {pair_count} ADTG/XML pairs"
        );
    } else {
        println!(
            "Native corpus verification ok: {xml_count} XML files, {adtg_count} ADTG files, {pair_count} ADTG/XML pairs, {adtg_only_count} ADTG-only artifacts"
        );
    }
    Ok(())
}

fn compare_native_command(
    left_path: PathBuf,
    right_path: PathBuf,
    mdac_resave_normalization: bool,
) -> Result<()> {
    let left = parse_recordset_file(&left_path)
        .with_context(|| format!("failed to parse {}", left_path.display()))?;
    let right = parse_recordset_file(&right_path)
        .with_context(|| format!("failed to parse {}", right_path.display()))?;
    let mismatches = if mdac_resave_normalization {
        compare_mdac_resaved_recordsets(&left, &right)
    } else {
        compare_native_recordsets(&left, &right)
    };
    if !mismatches.is_empty() {
        for mismatch in &mismatches {
            eprintln!("{mismatch}");
        }
        bail!(
            "native comparison failed for {} vs {} with {} mismatches",
            left_path.display(),
            right_path.display(),
            mismatches.len()
        );
    }

    println!(
        "Native comparison ok: {} vs {}",
        left_path.display(),
        right_path.display()
    );
    Ok(())
}

fn verify_native_adtg_only_artifact(path: &Path, recordset: &Recordset) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("native ADTG-only artifact path had no UTF-8 file name")?;

    match name {
        "orders_calc_new_pending_shape.adtg" => {
            verify_view_statuses(
                name,
                "default parent",
                &materialize_default_view(recordset),
                &[
                    RecordStatusFlag::Modified,
                    RecordStatusFlag::Modified,
                    RecordStatusFlag::Unmodified,
                ],
            )?;
            let pending = materialize_pending_view(recordset);
            verify_view_statuses(
                name,
                "pending parent",
                &pending,
                &[RecordStatusFlag::Modified, RecordStatusFlag::Modified],
            )?;
            for (index, row) in pending.rows.iter().enumerate() {
                let lines = chapter_value(&row.values, 5, name, "pending parent Lines")?;
                verify_view_statuses(
                    name,
                    &format!("pending parent {index} child pending"),
                    &materialize_pending_view(lines),
                    &[RecordStatusFlag::Modified],
                )?;
            }
        }
        "orders_pending_changes_shape.adtg" => {
            verify_view_statuses(
                name,
                "default parent",
                &materialize_default_view(recordset),
                &[RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
            )?;
            let pending = materialize_pending_view(recordset);
            verify_view_statuses(
                name,
                "pending parent",
                &pending,
                &[RecordStatusFlag::Modified],
            )?;
            let lines = chapter_value(&pending.rows[0].values, 3, name, "pending Lines")?;
            verify_view_statuses(
                name,
                "pending child",
                &materialize_pending_view(lines),
                &[
                    RecordStatusFlag::Deleted,
                    RecordStatusFlag::Modified,
                    RecordStatusFlag::New,
                ],
            )?;
        }
        "orders_parent_insert_delete_shape.adtg" => {
            verify_view_statuses(
                name,
                "default parent",
                &materialize_default_view(recordset),
                &[
                    RecordStatusFlag::Unmodified,
                    RecordStatusFlag::Unmodified,
                    RecordStatusFlag::New,
                ],
            )?;
            let pending = materialize_pending_view(recordset);
            verify_view_statuses(
                name,
                "pending parent",
                &pending,
                &[RecordStatusFlag::Deleted, RecordStatusFlag::New],
            )?;
            let deleted_lines = chapter_value(&pending.rows[0].values, 3, name, "deleted Lines")?;
            verify_view_len(
                name,
                "deleted parent child default",
                &materialize_default_view(deleted_lines),
                3,
            )?;
        }
        "orders_parent_relation_key_update_shape.adtg" => {
            let default = materialize_default_view(recordset);
            verify_view_statuses(
                name,
                "default parent",
                &default,
                &[RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
            )?;
            let updated_lines = chapter_value(&default.rows[0].values, 3, name, "updated Lines")?;
            verify_view_len(
                name,
                "updated parent old-key child default",
                &materialize_default_view(updated_lines),
                0,
            )?;
            let pending = materialize_pending_view(recordset);
            verify_view_statuses(
                name,
                "pending parent",
                &pending,
                &[RecordStatusFlag::Modified],
            )?;
            let pending_lines = chapter_value(&pending.rows[0].values, 3, name, "pending Lines")?;
            verify_view_len(
                name,
                "pending parent child default",
                &materialize_default_view(pending_lines),
                0,
            )?;
        }
        "orders_child_relation_key_update_shape.adtg" => {
            let default = materialize_default_view(recordset);
            verify_view_statuses(
                name,
                "default parent",
                &default,
                &[RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
            )?;
            verify_view_len(
                name,
                "top-level pending",
                &materialize_pending_view(recordset),
                0,
            )?;
            let raw_second = recordset
                .rows
                .get(1)
                .context("child relation-key update missing raw second parent row")?;
            let lines = chapter_value(&raw_second.values, 3, name, "raw second Lines")?;
            verify_view_statuses(
                name,
                "nested child pending",
                &materialize_pending_view(lines),
                &[RecordStatusFlag::Modified],
            )?;
        }
        "orders_composite_parent_relation_key_update_shape.adtg" => {
            let default = materialize_default_view(recordset);
            verify_view_statuses(
                name,
                "default parent",
                &default,
                &[RecordStatusFlag::Modified, RecordStatusFlag::Unmodified],
            )?;
            for (index, row) in default.rows.iter().enumerate() {
                let lines = chapter_value(&row.values, 2, name, "composite Lines")?;
                verify_view_len(
                    name,
                    &format!("default parent {index} composite child"),
                    &materialize_default_view(lines),
                    1,
                )?;
            }
            let pending = materialize_pending_view(recordset);
            verify_view_statuses(
                name,
                "pending parent",
                &pending,
                &[RecordStatusFlag::Modified],
            )?;
            let lines = chapter_value(&pending.rows[0].values, 2, name, "pending composite Lines")?;
            verify_view_len(
                name,
                "pending composite child default",
                &materialize_default_view(lines),
                1,
            )?;
        }
        "orders_composite_child_relation_key_update_shape.adtg" => {
            let default = materialize_default_view(recordset);
            verify_view_statuses(
                name,
                "default parent",
                &default,
                &[RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
            )?;
            verify_view_len(
                name,
                "top-level pending",
                &materialize_pending_view(recordset),
                0,
            )?;
            let old_key_lines = chapter_value(&default.rows[0].values, 2, name, "old-key Lines")?;
            verify_view_len(
                name,
                "old-key composite child default",
                &materialize_default_view(old_key_lines),
                0,
            )?;
            let raw_new_key = recordset
                .rows
                .get(1)
                .context("composite child relation-key update missing raw second parent row")?;
            let lines = chapter_value(&raw_new_key.values, 2, name, "raw new-key Lines")?;
            verify_view_statuses(
                name,
                "nested composite child pending",
                &materialize_pending_view(lines),
                &[RecordStatusFlag::Modified],
            )?;
        }
        "orders_lines_product_pending_shape.adtg" => {
            verify_nested_product_pending_adtg_only(name, recordset, 5, false)?;
        }
        "orders_lines_product_legacy_pending_shape.adtg" => {
            verify_nested_product_pending_adtg_only(name, recordset, 6, true)?;
        }
        _ => bail!("unrecognized native ADTG-only artifact"),
    }

    Ok(())
}

fn verify_nested_product_pending_adtg_only(
    name: &str,
    recordset: &Recordset,
    product_index: usize,
    has_legacy: bool,
) -> Result<()> {
    let default = materialize_default_view(recordset);
    verify_view_statuses(
        name,
        "default parent",
        &default,
        &[RecordStatusFlag::Unmodified, RecordStatusFlag::Unmodified],
    )?;
    verify_view_len(
        name,
        "top-level pending",
        &materialize_pending_view(recordset),
        0,
    )?;
    let first_lines = chapter_value(&default.rows[0].values, 3, name, "first Lines")?;
    let first_lines_default = materialize_default_view(first_lines);
    verify_view_statuses(
        name,
        "first parent default Lines",
        &first_lines_default,
        &[
            RecordStatusFlag::Modified,
            RecordStatusFlag::Unmodified,
            RecordStatusFlag::Unmodified,
        ],
    )?;

    let modified_product = chapter_value(
        &first_lines_default.rows[0].values,
        product_index,
        name,
        "modified Product",
    )?;
    verify_view_statuses(
        name,
        "modified Product default",
        &materialize_default_view(modified_product),
        &[RecordStatusFlag::Modified],
    )?;
    let deleted_product = chapter_value(
        &first_lines_default.rows[1].values,
        product_index,
        name,
        "deleted Product",
    )?;
    verify_view_len(
        name,
        "deleted Product default",
        &materialize_default_view(deleted_product),
        0,
    )?;
    verify_view_statuses(
        name,
        "deleted Product pending",
        &materialize_pending_view(deleted_product),
        &[RecordStatusFlag::Deleted],
    )?;

    if has_legacy {
        let legacy_index = product_index + 1;
        let modified_legacy = chapter_value(
            &first_lines_default.rows[0].values,
            legacy_index,
            name,
            "modified Legacy",
        )?;
        verify_view_statuses(
            name,
            "modified Legacy default",
            &materialize_default_view(modified_legacy),
            &[RecordStatusFlag::Modified],
        )?;
        let deleted_legacy = chapter_value(
            &first_lines_default.rows[1].values,
            legacy_index,
            name,
            "deleted Legacy",
        )?;
        verify_view_statuses(
            name,
            "deleted Legacy pending",
            &materialize_pending_view(deleted_legacy),
            &[RecordStatusFlag::Deleted],
        )?;
    }

    Ok(())
}

fn verify_view_statuses(
    artifact: &str,
    label: &str,
    view: &MaterializedRecordset,
    expected: &[RecordStatusFlag],
) -> Result<()> {
    let actual = view.rows.iter().map(|row| row.status).collect::<Vec<_>>();
    if actual != expected {
        bail!("{artifact}: {label} statuses were {actual:?}, expected {expected:?}");
    }
    Ok(())
}

fn verify_view_len(
    artifact: &str,
    label: &str,
    view: &MaterializedRecordset,
    expected: usize,
) -> Result<()> {
    if view.rows.len() != expected {
        bail!(
            "{artifact}: {label} row count was {}, expected {expected}",
            view.rows.len()
        );
    }
    Ok(())
}

fn chapter_value<'a>(
    values: &'a [Value],
    index: usize,
    artifact: &str,
    label: &str,
) -> Result<&'a Recordset> {
    match values.get(index) {
        Some(Value::Chapter(recordset)) => Ok(recordset),
        Some(other) => bail!("{artifact}: {label} at value {index} was not a chapter: {other:?}"),
        None => bail!("{artifact}: {label} at value {index} was missing"),
    }
}

fn corpus_recordset_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_recordset_paths(dir, &mut out)?;
    Ok(out)
}

fn collect_recordset_paths(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("failed to read {dir:?}"))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_recordset_paths(&path, out)?;
        } else if matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("xml" | "adtg")
        ) {
            out.push(path);
        }
    }
    Ok(())
}

fn native_comparison_xml_path(adtg_path: &Path) -> Option<PathBuf> {
    let file_stem = adtg_path.file_stem()?.to_str()?;
    let roundtrip = adtg_path.with_file_name(format!("{file_stem}.roundtrip.xml"));
    if roundtrip.exists() {
        return Some(roundtrip);
    }

    let same_stem = adtg_path.with_extension("xml");
    same_stem.exists().then_some(same_stem)
}

fn is_roundtrip_xml_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.ends_with(".roundtrip.xml"))
}

#[cfg(feature = "oracle")]
fn verify_com_command(input: PathBuf, cscript: Option<PathBuf>) -> Result<()> {
    verify_one_com(&input, cscript.as_deref(), true)?;
    Ok(())
}

#[cfg(feature = "oracle")]
fn verify_writer_com_command(
    input: PathBuf,
    cscript: Option<PathBuf>,
    keep_artifacts: bool,
) -> Result<()> {
    let source = parse_recordset_file(&input)?;
    let rust_xml_path = unique_temp_path("tablegram_writer_rust", "xml");
    let rust_adtg_path = unique_temp_path("tablegram_writer_rust", "adtg");
    let mdac_xml_from_xml = unique_temp_path("tablegram_writer_mdac_xml_from_xml", "xml");
    let mdac_adtg_from_xml = unique_temp_path("tablegram_writer_mdac_adtg_from_xml", "adtg");
    let mdac_xml_from_adtg = unique_temp_path("tablegram_writer_mdac_xml_from_adtg", "xml");
    let mdac_adtg_from_adtg = unique_temp_path("tablegram_writer_mdac_adtg_from_adtg", "adtg");
    let artifacts = [
        rust_xml_path.clone(),
        rust_adtg_path.clone(),
        mdac_xml_from_xml.clone(),
        mdac_adtg_from_xml.clone(),
        mdac_xml_from_adtg.clone(),
        mdac_adtg_from_adtg.clone(),
    ];

    let result: Result<()> = (|| {
        let xml = write_ado_xml(&source)
            .with_context(|| format!("failed to write Rust XML for {}", input.display()))?;
        std::fs::write(&rust_xml_path, xml)
            .with_context(|| format!("failed to write {:?}", rust_xml_path))?;

        let adtg = write_adtg(&source)
            .with_context(|| format!("failed to write Rust ADTG for {}", input.display()))?;
        std::fs::write(&rust_adtg_path, adtg)
            .with_context(|| format!("failed to write {:?}", rust_adtg_path))?;

        verify_writer_stage(&source, "rust xml", &rust_xml_path)?;
        verify_writer_stage(&source, "rust adtg", &rust_adtg_path)?;

        roundtrip_via_com(
            &rust_xml_path,
            &mdac_xml_from_xml,
            PersistFormat::Xml,
            cscript.as_deref(),
        )
        .context("MDAC failed to load Rust XML and save XML")?;
        roundtrip_via_com(
            &rust_xml_path,
            &mdac_adtg_from_xml,
            PersistFormat::Adtg,
            cscript.as_deref(),
        )
        .context("MDAC failed to load Rust XML and save ADTG")?;
        roundtrip_via_com(
            &rust_adtg_path,
            &mdac_xml_from_adtg,
            PersistFormat::Xml,
            cscript.as_deref(),
        )
        .context("MDAC failed to load Rust ADTG and save XML")?;
        roundtrip_via_com(
            &rust_adtg_path,
            &mdac_adtg_from_adtg,
            PersistFormat::Adtg,
            cscript.as_deref(),
        )
        .context("MDAC failed to load Rust ADTG and save ADTG")?;

        verify_writer_stage(&source, "mdac xml from rust xml", &mdac_xml_from_xml)?;
        verify_writer_stage(&source, "mdac adtg from rust xml", &mdac_adtg_from_xml)?;
        verify_writer_stage(&source, "mdac xml from rust adtg", &mdac_xml_from_adtg)?;
        verify_writer_stage(&source, "mdac adtg from rust adtg", &mdac_adtg_from_adtg)?;

        Ok(())
    })();

    if keep_artifacts {
        for artifact in &artifacts {
            eprintln!("writer COM artifact: {}", artifact.display());
        }
    } else {
        for artifact in &artifacts {
            let _ = std::fs::remove_file(artifact);
        }
    }

    result?;
    println!(
        "Writer COM verification ok: {} -> Rust XML/ADTG -> MDAC XML/ADTG cross-save",
        input.display()
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
#[cfg(feature = "oracle")]
enum PersistFormat {
    Xml,
    Adtg,
}

#[cfg(feature = "oracle")]
impl PersistFormat {
    fn script_arg(self) -> &'static str {
        match self {
            PersistFormat::Xml => "xml",
            PersistFormat::Adtg => "adtg",
        }
    }
}

#[cfg(feature = "oracle")]
fn verify_writer_stage(source: &Recordset, label: &str, path: &Path) -> Result<()> {
    let reparsed =
        parse_recordset_file(path).with_context(|| format!("failed to parse {label} output"))?;
    let mismatches = compare_native_recordsets(source, &reparsed);
    if mismatches.is_empty() {
        return Ok(());
    }

    for mismatch in &mismatches {
        eprintln!("{label}: {mismatch}");
    }
    bail!(
        "{label} verification failed with {} mismatches",
        mismatches.len()
    )
}

#[cfg(feature = "oracle")]
fn roundtrip_via_com(
    input: &Path,
    output_path: &Path,
    format: PersistFormat,
    cscript_override: Option<&Path>,
) -> Result<()> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join("roundtrip.vbs");
    let cscript = cscript_override
        .map(Path::to_path_buf)
        .unwrap_or_else(default_cscript_path);
    let _ = std::fs::remove_file(output_path);

    let output = ProcessCommand::new(&cscript)
        .arg("//nologo")
        .arg(&script)
        .arg(input)
        .arg(output_path)
        .arg(format.script_arg())
        .output()
        .with_context(|| format!("failed to run {:?}", cscript))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "COM roundtrip failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout,
            stderr
        );
    }

    if !output_path.exists() {
        bail!("COM roundtrip did not write {}", output_path.display());
    }
    Ok(())
}

#[cfg(feature = "oracle")]
fn verify_corpus_command(
    dir: PathBuf,
    cscript: Option<PathBuf>,
    jobs: usize,
    format: VerifyFormat,
) -> Result<()> {
    let mut paths = Vec::new();
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    for path in corpus_recordset_paths(&dir)? {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        let Some(extension) = extension.as_deref() else {
            continue;
        };
        if !matches!(extension, "xml" | "adtg") || !verify_format_allows(format, extension) {
            continue;
        }
        if is_documented_com_verification_skip_path(&path) {
            if let Err(err) = parse_input_for_verification(&path, cscript.as_deref()) {
                failures.push(format!(
                    "{}: documented COM verification skip failed native parse: {err:#}",
                    path.display()
                ));
            } else {
                skipped += 1;
            }
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    if paths.is_empty() {
        if !failures.is_empty() {
            for failure in &failures {
                eprintln!("{failure}");
            }
            bail!(
                "COM corpus verification failed: 0 passed, {} failed",
                failures.len()
            );
        }
        print_verification_summary(0, 0, format, skipped);
        return Ok(());
    }

    let jobs = jobs.max(1).min(paths.len());
    let queue = Arc::new(Mutex::new(paths));
    let results = Arc::new(Mutex::new(Vec::new()));
    let cscript = Arc::new(cscript);

    let mut handles = Vec::with_capacity(jobs);
    for _ in 0..jobs {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let cscript = Arc::clone(&cscript);
        handles.push(thread::spawn(move || -> std::result::Result<(), String> {
            loop {
                let path = {
                    let mut queue = queue
                        .lock()
                        .map_err(|_| String::from("verify queue lock poisoned"))?;
                    queue.pop()
                };
                let Some(path) = path else {
                    break;
                };

                let result = verify_one_com(&path, cscript.as_ref().as_deref(), false)
                    .map(|_| ())
                    .map_err(|err| format!("{}: {err:#}", path.display()));
                results
                    .lock()
                    .map_err(|_| String::from("verify result lock poisoned"))?
                    .push(result);
            }
            Ok(())
        }));
    }

    for handle in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => failures.push(err),
            Err(_) => failures.push(String::from("verify worker thread panicked")),
        }
    }

    let mut results = match results.lock() {
        Ok(mut results) => std::mem::take(&mut *results),
        Err(_) => {
            failures.push(String::from("verify result lock poisoned"));
            Vec::new()
        }
    };
    let checked = results.iter().filter(|result| result.is_ok()).count();
    failures.extend(results.drain(..).filter_map(Result::err));
    failures.sort();

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("{failure}");
        }
        bail!(
            "COM corpus verification failed: {} passed, {} failed",
            checked,
            failures.len()
        );
    }

    print_verification_summary(checked, jobs, format, skipped);
    Ok(())
}

#[cfg(feature = "oracle")]
fn print_verification_summary(checked: usize, jobs: usize, format: VerifyFormat, skipped: usize) {
    if skipped == 0 {
        println!("COM corpus verification ok: {checked} files ({jobs} jobs, {format:?})");
    } else {
        println!(
            "COM corpus verification ok: {checked} files ({jobs} jobs, {format:?}, {skipped} documented skips)"
        );
    }
}

#[cfg(feature = "oracle")]
fn verify_format_allows(format: VerifyFormat, extension: &str) -> bool {
    match format {
        VerifyFormat::All => true,
        VerifyFormat::Xml => extension == "xml",
        VerifyFormat::Adtg => extension == "adtg",
    }
}

#[cfg(feature = "oracle")]
fn verify_one_com(input: &Path, cscript: Option<&Path>, verbose: bool) -> Result<()> {
    let rust_recordset = parse_input_for_verification(input, cscript)?;
    let rust_default_view = materialize_default_view(&rust_recordset);
    let rust_pending_view = materialize_pending_view(&rust_recordset);
    let rust_affected_view = materialize_affected_view(&rust_recordset);
    let rust_conflicting_view = materialize_conflicting_view(&rust_recordset);
    let com_dump = dump_recordset_via_com(input, cscript)?;
    let compare_options = ComCompareOptions {
        ignore_key_column_flag: is_xml_input(input),
    };
    let mut mismatches = compare_com_view("none", &rust_default_view, &com_dump, compare_options);
    mismatches.extend(compare_com_view(
        "pending",
        &rust_pending_view,
        &com_dump,
        compare_options,
    ));
    mismatches.extend(compare_com_view(
        "affected",
        &rust_affected_view,
        &com_dump,
        compare_options,
    ));
    mismatches.extend(compare_com_view(
        "conflicting",
        &rust_conflicting_view,
        &com_dump,
        compare_options,
    ));

    if !mismatches.is_empty() {
        for mismatch in &mismatches {
            eprintln!("{mismatch}");
        }
        bail!(
            "COM verification failed with {} mismatches",
            mismatches.len()
        );
    }

    if verbose {
        println!(
            "COM verification ok: {} fields, {} default rows, {} pending rows",
            rust_default_view.fields.len(),
            rust_default_view.rows.len(),
            rust_pending_view.rows.len()
        );
    }
    Ok(())
}

#[cfg(feature = "oracle")]
fn is_xml_input(input: &Path) -> bool {
    input
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

#[cfg(feature = "oracle")]
fn parse_input_for_verification(input: &Path, _cscript: Option<&Path>) -> Result<Recordset> {
    parse_recordset_file(input)
}

#[cfg(feature = "oracle")]
fn parse_adtg_via_com(
    input: &Path,
    cscript_override: Option<&Path>,
) -> Result<tablegram::model::Recordset> {
    let dump = dump_recordset_via_com(input, cscript_override)?;
    recordset_from_com_dump(&dump)
}

#[cfg(feature = "oracle")]
fn recordset_from_com_dump(dump: &ComDump) -> Result<Recordset> {
    recordset_from_com_parts(&dump.fields, &dump.views)
}

#[cfg(feature = "oracle")]
fn recordset_from_com_parts(fields: &[ComField], views: &[ComView]) -> Result<Recordset> {
    let fields = fields
        .iter()
        .enumerate()
        .map(|(index, field)| com_field_to_field(index, field))
        .collect::<Vec<_>>();
    let default_view = views
        .iter()
        .find(|view| view.name == "none")
        .context("COM dump did not contain default view")?;
    let pending_view = views
        .iter()
        .find(|view| view.name == "pending")
        .context("COM dump did not contain pending view")?;

    let mut rows = Vec::new();
    let mut changes = Vec::new();

    for com_row in &default_view.rows {
        match status_from_code(com_row.status) {
            RecordStatusFlag::New => {
                push_com_change(
                    &fields,
                    &mut rows,
                    &mut changes,
                    RowChangeKind::Insert,
                    RowState::Inserted,
                    com_row,
                );
            }
            RecordStatusFlag::Modified => {
                let change_index = changes.len();
                let values = com_row_values(&fields, com_row);
                let original_index = rows.len();
                rows.push(Row {
                    ordinal: original_index,
                    state: RowState::Original,
                    status_flags: vec![RecordStatusFlag::Modified],
                    change_index: Some(change_index),
                    values: values.clone(),
                });
                let updated_index = rows.len();
                rows.push(Row {
                    ordinal: updated_index,
                    state: RowState::Updated,
                    status_flags: vec![RecordStatusFlag::Modified],
                    change_index: Some(change_index),
                    values,
                });
                changes.push(RowChange {
                    kind: RowChangeKind::Update,
                    row_indices: vec![original_index, updated_index],
                });
            }
            RecordStatusFlag::Deleted => {
                push_com_change(
                    &fields,
                    &mut rows,
                    &mut changes,
                    RowChangeKind::Delete,
                    RowState::Deleted,
                    com_row,
                );
            }
            RecordStatusFlag::Ok | RecordStatusFlag::Unmodified => {
                push_com_change(
                    &fields,
                    &mut rows,
                    &mut changes,
                    RowChangeKind::Current,
                    RowState::Current,
                    com_row,
                );
            }
        }
    }

    let missing_deleted = pending_view
        .rows
        .iter()
        .filter(|row| status_from_code(row.status) == RecordStatusFlag::Deleted)
        .count()
        .saturating_sub(
            changes
                .iter()
                .filter(|change| change.kind == RowChangeKind::Delete)
                .count(),
        );
    for _ in 0..missing_deleted {
        let change_index = changes.len();
        let row_index = rows.len();
        rows.push(Row {
            ordinal: row_index,
            state: RowState::Deleted,
            status_flags: vec![RecordStatusFlag::Deleted],
            change_index: Some(change_index),
            values: fields.iter().map(|_| Value::Unavailable).collect(),
        });
        changes.push(RowChange {
            kind: RowChangeKind::Delete,
            row_indices: vec![row_index],
        });
    }

    Ok(Recordset {
        fields,
        rows,
        changes,
    })
}

#[cfg(feature = "oracle")]
fn recordset_from_com_rows(fields: &[ComField], com_rows: &[ComRow]) -> Result<Recordset> {
    let fields = fields
        .iter()
        .enumerate()
        .map(|(index, field)| com_field_to_field(index, field))
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut changes = Vec::new();
    for com_row in com_rows {
        push_com_change(
            &fields,
            &mut rows,
            &mut changes,
            RowChangeKind::Current,
            RowState::Current,
            com_row,
        );
    }
    Ok(Recordset {
        fields,
        rows,
        changes,
    })
}

#[cfg(feature = "oracle")]
fn push_com_change(
    fields: &[Field],
    rows: &mut Vec<Row>,
    changes: &mut Vec<RowChange>,
    kind: RowChangeKind,
    state: RowState,
    com_row: &ComRow,
) {
    let change_index = changes.len();
    let row_index = rows.len();
    rows.push(Row {
        ordinal: row_index,
        state,
        status_flags: vec![status_from_code(com_row.status)],
        change_index: Some(change_index),
        values: com_row_values(fields, com_row),
    });
    changes.push(RowChange {
        kind,
        row_indices: vec![row_index],
    });
}

#[cfg(feature = "oracle")]
fn com_row_values(fields: &[Field], row: &ComRow) -> Vec<Value> {
    fields
        .iter()
        .zip(row.values.iter())
        .map(|(field, value)| value_from_com_value(field.ado_type.map(|ty| ty.code), value))
        .collect()
}

#[cfg(feature = "oracle")]
fn com_field_to_field(index: usize, field: &ComField) -> Field {
    let ado_type = ado_type_from_code(field.r#type);
    let attributes = field_attributes_from_code(field.attributes);
    Field {
        name: field.name.clone(),
        xml_name: field.name.clone(),
        ordinal: Some(index + 1),
        data_type: ado_type.map(|ty| ado_data_type_name(ty.code).to_string()),
        db_type: None,
        ado_type,
        max_length: (field.defined_size >= 0).then_some(field.defined_size as usize),
        precision: (field.precision >= 0).then_some(field.precision as usize),
        scale: (field.numeric_scale >= 0).then_some(field.numeric_scale as i32),
        nullable: attributes.contains(&FieldAttribute::IsNullable),
        writable: attributes.contains(&FieldAttribute::Updatable),
        fixed_length: matches!(
            field.r#type,
            2 | 3
                | 4
                | 5
                | 6
                | 7
                | 11
                | 14
                | 16
                | 17
                | 18
                | 19
                | 20
                | 21
                | 64
                | 72
                | 128
                | 129
                | 130
                | 131
                | 133
                | 134
                | 135
                | 136
        ),
        long: matches!(field.r#type, 201 | 203 | 205),
        key_column: field.attributes & 0x8000 != 0,
        base_catalog: None,
        base_schema: None,
        base_table: None,
        base_column: None,
        chapter_fields: None,
        chapter_relation: None,
        attributes,
    }
}

#[cfg(feature = "oracle")]
fn field_attributes_from_code(attributes: i64) -> Vec<FieldAttribute> {
    FieldAttribute::from_bits(attributes as u32)
}

#[cfg(feature = "oracle")]
fn value_from_com_value(field_type: Option<u16>, value: &ComValue) -> Value {
    match value {
        ComValue::Null => Value::Null,
        ComValue::Empty => Value::Empty,
        ComValue::Error { .. } => Value::Unavailable,
        ComValue::String { value } => Value::String(value.clone()),
        ComValue::Boolean { value } => Value::Boolean(*value),
        ComValue::Number { value } => match field_type {
            Some(2 | 3 | 16 | 20) => value
                .parse::<i64>()
                .map(Value::Integer)
                .unwrap_or_else(|_| Value::Decimal(value.clone())),
            Some(17 | 18 | 19 | 21) => value
                .parse::<u64>()
                .map(Value::UnsignedInteger)
                .unwrap_or_else(|_| Value::Decimal(value.clone())),
            Some(4 | 5) => value
                .parse::<f64>()
                .map(Value::Float)
                .unwrap_or_else(|_| Value::Decimal(value.clone())),
            _ => Value::Decimal(value.clone()),
        },
        ComValue::DateTime { value } => Value::DateTime(value.clone()),
        ComValue::BinaryHex { value } => Value::BinaryHex(value.clone()),
        ComValue::Chapter { fields, rows } => recordset_from_com_rows(fields, rows)
            .map(|recordset| Value::Chapter(Box::new(recordset)))
            .unwrap_or(Value::Unavailable),
    }
}

#[cfg(feature = "oracle")]
fn status_from_code(status: i64) -> RecordStatusFlag {
    match status {
        1 => RecordStatusFlag::New,
        2 => RecordStatusFlag::Modified,
        4 => RecordStatusFlag::Deleted,
        8 => RecordStatusFlag::Unmodified,
        _ => RecordStatusFlag::Ok,
    }
}

#[cfg(feature = "oracle")]
fn ado_type_from_code(code: i64) -> Option<AdoDataType> {
    Some(match code {
        0 => AdoDataType::new("adEmpty", 0),
        2 => AdoDataType::new("adSmallInt", 2),
        3 => AdoDataType::new("adInteger", 3),
        4 => AdoDataType::new("adSingle", 4),
        5 => AdoDataType::new("adDouble", 5),
        6 => AdoDataType::new("adCurrency", 6),
        7 => AdoDataType::new("adDate", 7),
        8 => AdoDataType::new("adBSTR", 8),
        10 => AdoDataType::new("adError", 10),
        11 => AdoDataType::new("adBoolean", 11),
        12 => AdoDataType::new("adVariant", 12),
        14 => AdoDataType::new("adDecimal", 14),
        16 => AdoDataType::new("adTinyInt", 16),
        17 => AdoDataType::new("adUnsignedTinyInt", 17),
        18 => AdoDataType::new("adUnsignedSmallInt", 18),
        19 => AdoDataType::new("adUnsignedInt", 19),
        20 => AdoDataType::new("adBigInt", 20),
        21 => AdoDataType::new("adUnsignedBigInt", 21),
        64 => AdoDataType::new("adFileTime", 64),
        72 => AdoDataType::new("adGUID", 72),
        128 => AdoDataType::new("adBinary", 128),
        129 => AdoDataType::new("adChar", 129),
        130 => AdoDataType::new("adWChar", 130),
        131 => AdoDataType::new("adNumeric", 131),
        133 => AdoDataType::new("adDBDate", 133),
        134 => AdoDataType::new("adDBTime", 134),
        135 => AdoDataType::new("adDBTimeStamp", 135),
        136 => AdoDataType::new("adChapter", 136),
        139 => AdoDataType::new("adVarNumeric", 139),
        200 => AdoDataType::new("adVarChar", 200),
        201 => AdoDataType::new("adLongVarChar", 201),
        202 => AdoDataType::new("adVarWChar", 202),
        203 => AdoDataType::new("adLongVarWChar", 203),
        204 => AdoDataType::new("adVarBinary", 204),
        205 => AdoDataType::new("adLongVarBinary", 205),
        _ => return None,
    })
}

#[cfg(feature = "oracle")]
fn ado_data_type_name(code: u16) -> &'static str {
    match code {
        2 | 3 | 16 | 20 => "int",
        17 | 18 | 19 | 21 => "uint",
        4 | 5 => "float",
        6 | 14 | 131 | 139 => "number",
        7 | 64 | 133 | 134 | 135 => "datetime",
        11 => "boolean",
        72 => "uuid",
        128 | 204 | 205 => "bin.hex",
        12 => "variant",
        136 => "chapter",
        _ => "string",
    }
}

#[cfg(feature = "oracle")]
fn dump_recordset_via_com(input: &Path, cscript_override: Option<&Path>) -> Result<ComDump> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools")
        .join("dump_recordset_json.vbs");
    let cscript = cscript_override
        .map(Path::to_path_buf)
        .unwrap_or_else(default_cscript_path);

    let output_json = unique_temp_path("tablegram_dump", "json");

    let output = ProcessCommand::new(&cscript)
        .arg("//nologo")
        .arg(&script)
        .arg(input)
        .arg(&output_json)
        .output()
        .with_context(|| format!("failed to run {:?}", cscript))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "COM dump failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            stdout,
            stderr
        );
    }

    let bytes = std::fs::read(&output_json)
        .with_context(|| format!("failed to read COM dump {:?}", output_json))?;
    let _ = std::fs::remove_file(&output_json);
    serde_json::from_slice(strip_utf8_bom(&bytes)).with_context(|| "failed to parse COM dump JSON")
}

#[cfg(feature = "oracle")]
fn default_cscript_path() -> PathBuf {
    let syswow64 = PathBuf::from(r"C:\Windows\SysWOW64\cscript.exe");
    if syswow64.exists() {
        syswow64
    } else {
        PathBuf::from("cscript.exe")
    }
}

#[cfg(feature = "oracle")]
fn default_job_count() -> usize {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .saturating_sub(2)
        .max(1)
}

#[cfg(feature = "oracle")]
fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}_{}_{}_{}.{}",
        std::process::id(),
        stamp,
        counter,
        extension
    ))
}

#[derive(Debug, Clone, Copy)]
#[cfg(feature = "oracle")]
struct ComCompareOptions {
    ignore_key_column_flag: bool,
}

#[cfg(feature = "oracle")]
fn compare_com_view(
    view_name: &str,
    rust: &MaterializedRecordset,
    com: &ComDump,
    options: ComCompareOptions,
) -> Vec<String> {
    let mut mismatches = Vec::new();
    if rust.fields.len() != com.fields.len() {
        mismatches.push(format!(
            "field count mismatch: rust={} com={}",
            rust.fields.len(),
            com.fields.len()
        ));
        return mismatches;
    }

    for (index, (rust_field, com_field)) in rust.fields.iter().zip(com.fields.iter()).enumerate() {
        if rust_field.name != com_field.name {
            mismatches.push(format!(
                "field {index} name mismatch: rust={:?} com={:?}",
                rust_field.name, com_field.name
            ));
        }
        if let Some(ado_type_code) = rust_field.ado_type_code {
            if ado_type_code as i64 != com_field.r#type {
                mismatches.push(format!(
                    "field {index} type mismatch: rust={} com={}",
                    ado_type_code, com_field.r#type
                ));
            }
        }
        if let Some(max_length) = rust_field.max_length {
            if max_length as i64 != com_field.defined_size {
                mismatches.push(format!(
                    "field {index} defined size mismatch: rust={} com={}",
                    max_length, com_field.defined_size
                ));
            }
        }
        if let Some(precision) = rust_field.precision {
            if precision as i64 != com_field.precision {
                mismatches.push(format!(
                    "field {index} precision mismatch: rust={} com={}",
                    precision, com_field.precision
                ));
            }
        }
        if let Some(scale) = rust_field.scale {
            if scale as i64 != com_field.numeric_scale {
                mismatches.push(format!(
                    "field {index} numeric scale mismatch: rust={} com={}",
                    scale, com_field.numeric_scale
                ));
            }
        }
        if comparable_com_attribute_flags(rust_field.attribute_flags, options)
            != comparable_com_attribute_flags_i64(com_field.attributes, options)
        {
            mismatches.push(format!(
                "field {index} attributes mismatch: rust={} com={}",
                rust_field.attribute_flags, com_field.attributes
            ));
        }
    }

    let Some(com_view) = com.views.iter().find(|view| view.name == view_name) else {
        mismatches.push(format!("COM dump did not contain {view_name:?} view"));
        return mismatches;
    };

    if rust.rows.len() != com_view.rows.len() {
        mismatches.push(format!(
            "{view_name} row count mismatch: rust={} com={}",
            rust.rows.len(),
            com_view.rows.len()
        ));
        return mismatches;
    }

    if view_name == "pending" {
        mismatches.extend(compare_rows_unordered(view_name, rust, com_view, options));
    } else {
        mismatches.extend(compare_rows_ordered(view_name, rust, com_view, options));
    }

    mismatches
}

#[cfg(feature = "oracle")]
fn comparable_com_attribute_flags(flags: u32, options: ComCompareOptions) -> i64 {
    if options.ignore_key_column_flag {
        (flags & !0x8000) as i64
    } else {
        flags as i64
    }
}

#[cfg(feature = "oracle")]
fn comparable_com_attribute_flags_i64(flags: i64, options: ComCompareOptions) -> i64 {
    if options.ignore_key_column_flag {
        flags & !0x8000
    } else {
        flags
    }
}

#[cfg(feature = "oracle")]
fn compare_rows_ordered(
    view_name: &str,
    rust: &MaterializedRecordset,
    com_view: &ComView,
    options: ComCompareOptions,
) -> Vec<String> {
    let mut mismatches = Vec::new();

    for (row_index, (rust_row, com_row)) in rust.rows.iter().zip(com_view.rows.iter()).enumerate() {
        let expected_status = record_status_code(rust_row.status);
        if expected_status != com_row.status {
            mismatches.push(format!(
                "{view_name} row {row_index} status mismatch: rust={} com={}",
                expected_status, com_row.status
            ));
        }

        if rust_row.values.len() != com_row.values.len() {
            mismatches.push(format!(
                "{view_name} row {row_index} value count mismatch: rust={} com={}",
                rust_row.values.len(),
                com_row.values.len()
            ));
        }

        for (field_index, (rust_value, com_value)) in rust_row
            .values
            .iter()
            .zip(com_row.values.iter())
            .enumerate()
        {
            if rust_row.status == RecordStatusFlag::Deleted
                && matches!(com_value, ComValue::Error { .. })
            {
                continue;
            }
            if !value_matches(rust_value, com_value, options) {
                mismatches.push(format!(
                    "{view_name} row {row_index} field {field_index} value mismatch: rust={rust_value:?} com={com_value:?}"
                ));
            }
        }
    }

    mismatches
}

#[cfg(feature = "oracle")]
fn compare_rows_unordered(
    view_name: &str,
    rust: &MaterializedRecordset,
    com_view: &ComView,
    options: ComCompareOptions,
) -> Vec<String> {
    let mut mismatches = Vec::new();
    let mut unmatched = vec![true; rust.rows.len()];

    for (com_row_index, com_row) in com_view.rows.iter().enumerate() {
        let Some(rust_row_index) = rust.rows.iter().enumerate().find_map(|(index, rust_row)| {
            (unmatched[index] && row_matches_com(rust_row, com_row, options)).then_some(index)
        }) else {
            mismatches.push(format!(
                "{view_name} COM row {com_row_index} did not match any Rust row: status={} values={:?}",
                com_row.status, com_row.values
            ));
            continue;
        };
        unmatched[rust_row_index] = false;
    }

    for (index, rust_row) in rust.rows.iter().enumerate() {
        if unmatched[index] {
            mismatches.push(format!(
                "{view_name} Rust row {index} did not match any COM row: status={} values={:?}",
                record_status_code(rust_row.status),
                rust_row.values
            ));
        }
    }

    mismatches
}

#[cfg(feature = "oracle")]
fn row_matches_com(
    rust_row: &tablegram::compat::MaterializedRow,
    com_row: &ComRow,
    options: ComCompareOptions,
) -> bool {
    if record_status_code(rust_row.status) != com_row.status {
        return false;
    }
    if rust_row.values.len() != com_row.values.len() {
        return false;
    }

    rust_row
        .values
        .iter()
        .zip(com_row.values.iter())
        .all(|(rust_value, com_value)| {
            if rust_row.status == RecordStatusFlag::Deleted
                && matches!(com_value, ComValue::Error { .. })
            {
                true
            } else {
                value_matches(rust_value, com_value, options)
            }
        })
}

#[cfg(feature = "oracle")]
fn record_status_code(status: RecordStatusFlag) -> i64 {
    match status {
        RecordStatusFlag::Ok => 0,
        RecordStatusFlag::New => 1,
        RecordStatusFlag::Modified => 2,
        RecordStatusFlag::Deleted => 4,
        RecordStatusFlag::Unmodified => 8,
    }
}

#[cfg(feature = "oracle")]
fn value_matches(rust: &Value, com: &ComValue, options: ComCompareOptions) -> bool {
    match (rust, com) {
        (Value::Null, ComValue::Null) => true,
        (Value::Empty, ComValue::Empty) => true,
        (Value::String(left), ComValue::String { value, .. }) => left == value,
        (Value::Guid(left), ComValue::String { value, .. }) => left.eq_ignore_ascii_case(value),
        (Value::Boolean(left), ComValue::Boolean { value, .. }) => *left == *value,
        (Value::Integer(left), ComValue::Number { value, .. }) => value
            .parse::<i64>()
            .map(|right| *left == right)
            .unwrap_or(false),
        (Value::UnsignedInteger(left), ComValue::Number { value, .. }) => value
            .parse::<u64>()
            .map(|right| *left == right)
            .unwrap_or(false),
        (Value::Float(left), ComValue::Number { value, .. }) => value
            .parse::<f64>()
            .map(|right| float_values_match(*left, right))
            .unwrap_or(false),
        (Value::Decimal(left), ComValue::Number { value, .. }) => numeric_text_matches(left, value),
        (Value::Date(left), ComValue::DateTime { value, .. }) => {
            value == &format!("{left}T00:00:00")
        }
        (Value::Time(left), ComValue::DateTime { value, .. }) => {
            value.ends_with(&format!("T{left}"))
        }
        (Value::DateTime(left), ComValue::DateTime { value, .. }) => {
            canonical_datetime_text(left) == canonical_datetime_text(value)
        }
        (Value::BinaryHex(left), ComValue::BinaryHex { value, .. }) => {
            left.eq_ignore_ascii_case(value)
        }
        (Value::Chapter(left), ComValue::Chapter { fields, rows }) => {
            let materialized = materialize_default_view(left);
            com_fields_match(&materialized.fields, fields, options)
                && com_rows_match_ordered(&materialized.rows, rows, options)
        }
        _ => false,
    }
}

#[cfg(feature = "oracle")]
fn com_fields_match(
    fields: &[tablegram::compat::MaterializedField],
    com: &[ComField],
    options: ComCompareOptions,
) -> bool {
    fields.len() == com.len()
        && fields.iter().zip(com.iter()).all(|(field, com_field)| {
            field.name == com_field.name
                && field
                    .ado_type_code
                    .map(|code| code as i64 == com_field.r#type)
                    .unwrap_or(true)
                && field
                    .max_length
                    .map(|max_length| max_length as i64 == com_field.defined_size)
                    .unwrap_or(true)
                && field
                    .precision
                    .map(|precision| precision as i64 == com_field.precision)
                    .unwrap_or(true)
                && field
                    .scale
                    .map(|scale| scale as i64 == com_field.numeric_scale)
                    .unwrap_or(true)
                && comparable_com_attribute_flags(field.attribute_flags, options)
                    == comparable_com_attribute_flags_i64(com_field.attributes, options)
        })
}

#[cfg(feature = "oracle")]
fn com_rows_match_ordered(
    rows: &[tablegram::compat::MaterializedRow],
    com: &[ComRow],
    options: ComCompareOptions,
) -> bool {
    rows.len() == com.len()
        && rows
            .iter()
            .zip(com.iter())
            .all(|(row, com_row)| row_matches_com(row, com_row, options))
}

#[cfg(feature = "oracle")]
fn float_values_match(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 0.000001
}

#[cfg(feature = "oracle")]
fn numeric_text_matches(left: &str, right: &str) -> bool {
    canonical_decimal_text(left) == canonical_decimal_text(right)
}

#[cfg(feature = "oracle")]
fn canonical_datetime_text(raw: &str) -> String {
    let Some((head, fraction)) = raw.split_once('.') else {
        return raw.to_string();
    };
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        head.to_string()
    } else {
        format!("{head}.{fraction}")
    }
}

#[cfg(feature = "oracle")]
fn canonical_decimal_text(raw: &str) -> String {
    let trimmed = raw.trim();
    let (negative, body) = trimmed
        .strip_prefix('-')
        .map(|body| (true, body))
        .unwrap_or((false, trimmed));
    let (whole, fraction) = body
        .split_once('.')
        .map(|(whole, fraction)| (whole, Some(fraction)))
        .unwrap_or((body, None));
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = fraction.map(|value| value.trim_end_matches('0'));
    match fraction {
        Some(fraction) if !fraction.is_empty() && negative => format!("-{whole}.{fraction}"),
        Some(fraction) if !fraction.is_empty() => format!("{whole}.{fraction}"),
        _ if negative && whole != "0" => format!("-{whole}"),
        _ => whole.to_string(),
    }
}

#[derive(Debug, Deserialize)]
#[cfg(feature = "oracle")]
struct ComDump {
    fields: Vec<ComField>,
    views: Vec<ComView>,
}

#[derive(Debug, Deserialize)]
#[cfg(feature = "oracle")]
struct ComField {
    name: String,
    #[serde(rename = "type")]
    r#type: i64,
    #[serde(default)]
    defined_size: i64,
    #[serde(default)]
    attributes: i64,
    #[serde(default)]
    precision: i64,
    #[serde(default)]
    numeric_scale: i64,
}

#[derive(Debug, Deserialize)]
#[cfg(feature = "oracle")]
struct ComView {
    name: String,
    rows: Vec<ComRow>,
}

#[derive(Debug, Deserialize)]
#[cfg(feature = "oracle")]
struct ComRow {
    status: i64,
    values: Vec<ComValue>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg(feature = "oracle")]
enum ComValue {
    Null,
    Empty,
    Error {
        #[serde(rename = "error")]
        _error: serde_json::Value,
    },
    String {
        value: String,
    },
    Boolean {
        value: bool,
    },
    Number {
        value: String,
    },
    DateTime {
        value: String,
    },
    BinaryHex {
        value: String,
    },
    Chapter {
        fields: Vec<ComField>,
        rows: Vec<ComRow>,
    },
}

fn hexdiff_command(left: PathBuf, right: PathBuf, width: usize, max_lines: usize) -> Result<()> {
    let left_bytes = std::fs::read(&left).with_context(|| format!("failed to read {left:?}"))?;
    let right_bytes = std::fs::read(&right).with_context(|| format!("failed to read {right:?}"))?;
    let diff = hexdiff(
        &left_bytes,
        &right_bytes,
        HexDiffOptions { width, max_lines },
    );
    print!("{diff}");
    Ok(())
}

fn print_adtg_summary(document: &tablegram::adtg::AdtgDocument) {
    println!("ADTG binary: {} bytes", document.length);
    println!("first_u32_le: {:?}", document.first_u32_le);
    println!("header_hex: {}", document.header_hex);
    println!("trailer_hex: {}", document.trailer_hex);
    println!("detected strings:");
    for item in document.detected_strings.iter().take(40) {
        println!("  {:08X} {:?}: {}", item.offset, item.encoding, item.text);
    }
}

#[cfg(test)]
mod tests {
    use super::corpus_recordset_paths;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn corpus_recordset_paths_recurses_into_nested_corpora() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tablegram_recursive_collect_{}_{}",
            std::process::id(),
            unique
        ));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("top.xml"), b"").unwrap();
        fs::write(nested.join("child.adtg"), b"").unwrap();
        fs::write(nested.join("ignored.txt"), b"").unwrap();

        let mut paths = corpus_recordset_paths(&root)
            .unwrap()
            .into_iter()
            .map(|path| {
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        paths.sort();

        fs::remove_dir_all(&root).unwrap();
        assert_eq!(paths, vec!["nested/child.adtg", "top.xml"]);
    }
}
