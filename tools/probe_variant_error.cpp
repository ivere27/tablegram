#include <windows.h>
#include <oleauto.h>

#include <iostream>
#include <string>
#include <vector>

namespace {

struct ComInit {
    ComInit() { hr = CoInitialize(nullptr); }
    ~ComInit() {
        if (SUCCEEDED(hr)) {
            CoUninitialize();
        }
    }
    HRESULT hr;
};

struct VariantHolder {
    VARIANT value;
    VariantHolder() { VariantInit(&value); }
    ~VariantHolder() { VariantClear(&value); }
    VARIANT *operator&() { return &value; }
};

std::wstring widen(const char *text) {
    int needed = MultiByteToWideChar(CP_UTF8, 0, text, -1, nullptr, 0);
    std::wstring out(static_cast<size_t>(needed - 1), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, text, -1, out.data(), needed);
    return out;
}

void check(HRESULT hr, const char *what) {
    if (FAILED(hr)) {
        std::cerr << what << " failed: 0x" << std::hex << static_cast<unsigned long>(hr) << "\n";
        ExitProcess(static_cast<UINT>(hr));
    }
}

DISPID dispid(IDispatch *object, const wchar_t *name) {
    LPOLESTR names[] = {const_cast<LPOLESTR>(name)};
    DISPID id = 0;
    check(object->GetIDsOfNames(IID_NULL, names, 1, LOCALE_USER_DEFAULT, &id), "GetIDsOfNames");
    return id;
}

VariantHolder invoke(
    IDispatch *object,
    const wchar_t *name,
    WORD flags,
    std::vector<VARIANT> args = {},
    bool property_put = false) {
    DISPID id = dispid(object, name);
    DISPPARAMS params{};
    params.cArgs = static_cast<UINT>(args.size());
    params.rgvarg = args.empty() ? nullptr : args.data();
    DISPID put_id = DISPID_PROPERTYPUT;
    if (property_put) {
        params.cNamedArgs = 1;
        params.rgdispidNamedArgs = &put_id;
    }
    VariantHolder result;
    EXCEPINFO excep{};
    UINT arg_err = 0;
    HRESULT hr = object->Invoke(
        id,
        IID_NULL,
        LOCALE_USER_DEFAULT,
        flags,
        &params,
        &result.value,
        &excep,
        &arg_err);
    if (FAILED(hr)) {
        std::wcerr << L"Invoke " << name << L" failed: 0x" << std::hex << static_cast<unsigned long>(hr);
        if (excep.bstrDescription) {
            std::wcerr << L" " << excep.bstrDescription;
        }
        std::wcerr << L"\n";
        ExitProcess(static_cast<UINT>(hr));
    }
    return result;
}

VARIANT v_i4(LONG value) {
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_I4;
    v.lVal = value;
    return v;
}

VARIANT v_bstr(const std::wstring &value) {
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_BSTR;
    v.bstrVal = SysAllocString(value.c_str());
    return v;
}

VARIANT v_error(SCODE value) {
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_ERROR;
    v.scode = value;
    return v;
}

IDispatch *create_recordset() {
    CLSID clsid{};
    check(CLSIDFromProgID(L"ADODB.Recordset", &clsid), "CLSIDFromProgID");
    IDispatch *recordset = nullptr;
    check(
        CoCreateInstance(clsid, nullptr, CLSCTX_INPROC_SERVER, IID_IDispatch, reinterpret_cast<void **>(&recordset)),
        "CoCreateInstance");
    return recordset;
}

IDispatch *as_dispatch(VARIANT &value) {
    if (value.vt == VT_DISPATCH && value.pdispVal) {
        value.pdispVal->AddRef();
        return value.pdispVal;
    }
    std::cerr << "expected IDispatch result\n";
    ExitProcess(2);
}

IDispatch *fields_of(IDispatch *recordset) {
    VariantHolder fields = invoke(recordset, L"Fields", DISPATCH_PROPERTYGET);
    return as_dispatch(fields.value);
}

IDispatch *field_item(IDispatch *recordset, const wchar_t *name) {
    IDispatch *fields = fields_of(recordset);
    std::vector<VARIANT> args;
    args.push_back(v_bstr(name));
    VariantHolder item = invoke(fields, L"Item", DISPATCH_PROPERTYGET, args);
    fields->Release();
    return as_dispatch(item.value);
}

void put_value(IDispatch *recordset, const wchar_t *name, VARIANT value) {
    IDispatch *field = field_item(recordset, name);
    std::vector<VARIANT> args;
    args.push_back(value);
    invoke(field, L"Value", DISPATCH_PROPERTYPUT, args, true);
    VariantClear(&args[0]);
    field->Release();
}

void append_field(IDispatch *recordset, const wchar_t *name, int type, int size, int attributes) {
    IDispatch *fields = fields_of(recordset);
    std::vector<VARIANT> args;
    args.push_back(v_i4(attributes));
    args.push_back(v_i4(size));
    args.push_back(v_i4(type));
    args.push_back(v_bstr(name));
    invoke(fields, L"Append", DISPATCH_METHOD, args);
    for (auto &arg : args) {
        VariantClear(&arg);
    }
    fields->Release();
}

void add_error_row(IDispatch *recordset, int row_id, int value_index) {
    static const SCODE errors[] = {2000, 2001, 2002, 2003, 2004};
    invoke(recordset, L"AddNew", DISPATCH_METHOD);
    put_value(recordset, L"ID", v_i4(row_id));
    put_value(recordset, L"VALUE_FIELD", v_error(errors[value_index % 5]));
    invoke(recordset, L"Update", DISPATCH_METHOD);
}

void save(IDispatch *recordset, const std::wstring &path, int format) {
    std::vector<VARIANT> args;
    args.push_back(v_i4(format));
    args.push_back(v_bstr(path));
    invoke(recordset, L"Save", DISPATCH_METHOD, args);
    for (auto &arg : args) {
        VariantClear(&arg);
    }
}

void roundtrip_adtg_to_xml(const std::wstring &adtg, const std::wstring &xml) {
    IDispatch *recordset = create_recordset();
    std::vector<VARIANT> args;
    args.push_back(v_i4(256));
    args.push_back(v_i4(4));
    args.push_back(v_i4(3));
    args.push_back(v_bstr(L"Provider=MSPersist"));
    args.push_back(v_bstr(adtg));
    invoke(recordset, L"Open", DISPATCH_METHOD, args);
    for (auto &arg : args) {
        VariantClear(&arg);
    }
    save(recordset, xml, 1);
    invoke(recordset, L"Close", DISPATCH_METHOD);
    recordset->Release();
}

}  // namespace

int main(int argc, char **argv) {
    if (argc != 2) {
        std::cerr << "usage: probe_variant_error.exe output-dir\n";
        return 2;
    }

    ComInit com;
    check(com.hr, "CoInitialize");

    std::wstring root = widen(argv[1]);
    CreateDirectoryW(root.c_str(), nullptr);
    std::wstring xml = root + L"\\variant_error.xml";
    std::wstring adtg = root + L"\\variant_error.adtg";
    std::wstring roundtrip = root + L"\\variant_error.roundtrip.xml";
    DeleteFileW(xml.c_str());
    DeleteFileW(adtg.c_str());
    DeleteFileW(roundtrip.c_str());

    IDispatch *recordset = create_recordset();
    std::vector<VARIANT> cursor_args;
    cursor_args.push_back(v_i4(3));
    invoke(recordset, L"CursorLocation", DISPATCH_PROPERTYPUT, cursor_args, true);
    append_field(recordset, L"ID", 3, 0, 0);
    append_field(recordset, L"VALUE_FIELD", 12, 0, 32);
    invoke(recordset, L"Open", DISPATCH_METHOD);

    add_error_row(recordset, 1, 0);
    add_error_row(recordset, 2, 1);
    add_error_row(recordset, 3, 2);
    std::vector<VARIANT> batch_args;
    batch_args.push_back(v_i4(3));
    invoke(recordset, L"UpdateBatch", DISPATCH_METHOD, batch_args);

    invoke(recordset, L"MoveFirst", DISPATCH_METHOD);
    put_value(recordset, L"VALUE_FIELD", v_error(2003));
    invoke(recordset, L"Update", DISPATCH_METHOD);
    invoke(recordset, L"MoveNext", DISPATCH_METHOD);
    invoke(recordset, L"Delete", DISPATCH_METHOD);
    add_error_row(recordset, 4, 4);

    VariantHolder clone_variant = invoke(recordset, L"Clone", DISPATCH_METHOD);
    IDispatch *clone = as_dispatch(clone_variant.value);
    save(recordset, xml, 1);
    save(clone, adtg, 0);
    invoke(clone, L"Close", DISPATCH_METHOD);
    invoke(recordset, L"Close", DISPATCH_METHOD);
    clone->Release();
    recordset->Release();

    roundtrip_adtg_to_xml(adtg, roundtrip);
    std::wcout << L"ok\n" << xml << L"\n" << adtg << L"\n";
    return 0;
}
