#![no_main]

use tablegram::adtg::{inspect_adtg, parse_adtg_bytes};
use tablegram::detect::{detect_format, RecordsetFormat};
use tablegram::xml::parse_ado_xml_bytes;
use tablegram::parse_recordset_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_recordset_bytes(data);
    let _ = parse_ado_xml_bytes(data);

    if detect_format(data) == RecordsetFormat::Adtg {
        let _ = inspect_adtg(data);
        let _ = parse_adtg_bytes(data);
    }
});
