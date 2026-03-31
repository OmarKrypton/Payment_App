import streamlit as st
from openpyxl import Workbook
from openpyxl.styles import Font, Alignment, Border, Side, PatternFill
from datetime import datetime
import os
import json
import io

st.set_page_config(page_title="CSCEC - Payment Voucher Settlement", layout="wide")

DATA_DIR = "data"
os.makedirs(DATA_DIR, exist_ok=True)
CONFIG_FILE = os.path.join(DATA_DIR, "config.json")
LOGO_PATH = "assets/cscec.png"
CSCEC_BLUE = "#0066B3"
CSCEC_DARK = "#000000"

def load_config():
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, 'r') as f:
            return json.load(f)
    return {}

def save_config(config):
    with open(CONFIG_FILE, 'w') as f:
        json.dump(config, f, indent=2)

def export_to_excel(data):
    wb = Workbook()
    ws = wb.active
    ws.title = "Settlement Report"
    thin = Side(style='thin')
    thin_border = Border(left=thin, right=thin, top=thin, bottom=thin)
    header_fill = PatternFill(start_color="0066B3", end_color="0066B3", fill_type="solid")
    calc_fill = PatternFill(start_color="E2EFDA", end_color="E2EFDA", fill_type="solid")
    
    ws.column_dimensions['A'].width = 8
    ws.column_dimensions['B'].width = 75
    ws.column_dimensions['C'].width = 25
    
    ws['B1'] = "CSCEC - Payment Voucher Settlement"
    ws['B1'].fill = header_fill
    ws['B1'].font = Font(bold=True, color="FFFFFF", size=12)
    
    row = 3
    labels = [
        ("1", "Initial accumulative supplier settlement amount", "1A"),
        ("", "Current period supplier settlement without VAT", "1B"),
        ("", "Current period other-payables under contract", "1C"),
        ("", "Current period other-deductions under contract", "1D"),
        ("", "Current period VAT of supplier settlement amount", "1E"),
        ("", "Ending accumulative supplier settlement amount", "1F"),
        ("", "", ""),
        ("2", "Advance payment under contract", "2A"),
        ("", "Initial deduction from advance payment", "2B"),
        ("", "Current period deduction from advance payment", "2C"),
        ("", "Ending deduction from advance payment", "2D"),
        ("", "Ending balance of advance payment", "2E"),
        ("", "", ""),
        ("3", "Ending accumulative amount payable", "3A"),
        ("", "", ""),
        ("4", "Initial balance of retention amount", "4A"),
        ("", "Current period retention money to be deducted", "4B"),
        ("", "Current period return of retention money", "4C"),
        ("", "Ending balance of retention money", "4D"),
        ("", "", ""),
        ("5", "Initial balance of temporary labour social insurance", "5A"),
        ("", "Current temporary labour social insurance to deduct", "5B"),
        ("", "Current return of temporary labour social insurance", "5C"),
        ("", "Ending balance of temporary labour social insurance", "5D"),
        ("", "", ""),
        ("6", "Initial accumulative WHT", "6A"),
        ("", "Current period WHT", "6B"),
        ("", "Ending accumulative WHT", "6C"),
        ("", "", ""),
        ("7", "Initial accumulative reality amount paid", "7A"),
        ("", "Initial total accumulative paid amount", "7B"),
        ("", "", ""),
        ("8", "Initial other-deductions", "8A"),
        ("", "Current other-deductions", "8B"),
        ("", "Ending accumulative other-deductions", "8C"),
        ("", "", ""),
        ("10", "Current period reality amount paid", "10A"),
        ("", "", ""),
        ("11", "Ending accumulative reality amount paid", "11A"),
        ("", "Ending total accumulative amount paid", "11B"),
    ]
    
    for section, desc, key in labels:
        if desc == "":
            row += 1
            continue
            
        ws[f'A{row}'] = section
        ws[f'B{row}'] = desc
        ws[f'C{row}'] = data.get(key, 0.0)
        
        # highlighting logic
        if key in ["1F", "2D", "2E", "3A", "4D", "5D", "6C", "7B", "8C", "11A", "11B"]:
            ws[f'C{row}'].fill = calc_fill
            
        row += 1

    virtual_workbook = io.BytesIO()
    wb.save(virtual_workbook)
    virtual_workbook.seek(0)
    return virtual_workbook.getvalue()

st.markdown(f"""
<style>
    /* Remove all number input arrows */
    input[type="number"]::-webkit-outer-spin-button,
    input[type="number"]::-webkit-inner-spin-button {{ -webkit-appearance: none !important; margin: 0 !important; }}
    input[type="number"] {{ -moz-appearance: textfield !important; appearance: none !important; }}
    
    /* Strict CSS Grid / Border Styles imitating CSCEC Paper Form dynamically adapting to Themes */
    div[data-testid="stVerticalBlock"] > div[style*="border-color"] {{ border: 2px solid var(--text-color) !important; border-radius: 0 !important; background-color: var(--secondary-background-color); }}
    input[type="text"] {{ border: 1px solid var(--text-color) !important; border-radius: 0 !important; background-color: var(--background-color) !important; color: var(--text-color) !important; font-weight: 600 !important; font-family: monospace; opacity: 0.85; }}
    input[type="text"]:focus {{ border-color: {CSCEC_BLUE} !important; opacity: 1.0; box-shadow: none !important; }}
    .stSelectbox > div > div {{ border: 1px solid var(--text-color) !important; border-radius: 0 !important; background-color: var(--background-color) !important; color: var(--text-color) !important; opacity: 0.85; }}
    
    .section-title {{ color: var(--text-color); font-size: 16px; font-weight: 800; margin-bottom: 15px; padding-bottom: 6px; border-bottom: 2px solid var(--text-color); text-transform: uppercase; font-family: "Microsoft YaHei", sans-serif; }}
    .cn-label {{ font-size: 14px; font-weight: 700; color: var(--text-color); margin-bottom: 2px; font-family: "Microsoft YaHei", sans-serif; line-height: 1.2; opacity: 0.95; }}
    .en-label {{ font-size: 11px; color: var(--text-color); margin-bottom: 4px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; line-height: 1.1; opacity: 0.7; }}
    
    .calc-value {{ background: rgba(0,102,179,0.1); padding: 6px 12px; border-radius: 0px; font-size: 14px; font-weight: 700; color: var(--text-color); border: 1px solid var(--text-color); margin-bottom: 12px; font-family: monospace; text-align: right; letter-spacing: 0.5px; opacity: 0.9; }}
    
    .result-final {{ background: var(--background-color); padding: 15px 20px; border: 2px solid var(--text-color); color: var(--text-color); text-align: center; margin-top: 10px; margin-bottom: 20px; }}
    
    /* Global Adjustments */
    .stApp > header {{ display: none; }}
</style>
""", unsafe_allow_html=True)

def main():
    config = load_config()
    if 'config' not in st.session_state:
        st.session_state.config = config
    
    colA, colB = st.columns([1, 15])
    with colA:
        try: st.image(LOGO_PATH, width=60)
        except: st.markdown(f"<b style='color:var(--text-color); font-size:14px; font-weight: 800;'>CSCEC</b>", unsafe_allow_html=True)
    with colB:
        st.markdown(f"""
        <div style="background: var(--background-color); border: 2px solid var(--text-color); padding: 10px 20px;">
            <div style="color: var(--text-color); font-size: 18px; font-weight: 800; font-family: 'Microsoft YaHei', sans-serif;">Payment Voucher for Approval | 中国建筑管理表格</div>
            <div style="color: var(--text-color); opacity: 0.7; font-size: 12px; text-transform: uppercase; font-weight:600;">China State Construction Engineering Corporation</div>
        </div>
        """, unsafe_allow_html=True)

    def get_val(key, default=0.0): return st.session_state.config.get(key, default)
    def save_val(key, value): st.session_state.config[key] = value

    def num_input(zh, en, key, default_val=0.0):
        st.markdown(f'<div class="cn-label">{zh}</div><div class="en-label">{en}</div>', unsafe_allow_html=True)
        val = st.text_input("hidden", value=str(get_val(key, default_val)), key=f"n_{key}", label_visibility="collapsed", on_change=lambda: save_val(key, float(st.session_state[f"n_{key}"] or 0)))
        val = float(val or 0)
        save_val(key, val)
        st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
        return val

    def calc_display(zh, en, value):
        st.markdown(f'<div class="cn-label">{zh}</div><div class="en-label">{en}</div>', unsafe_allow_html=True)
        st.markdown(f'<div class="calc-value">EGP {value:,.2f}</div>', unsafe_allow_html=True)

    # 1. SUPPLIER SETTLEMENT
    with st.container(border=True):
        st.markdown('<div class="section-title">供应商结算 | Supplier Settlement</div>', unsafe_allow_html=True)
        col1, col2 = st.columns(2)
        with col1:
            val_1A = num_input("期初累计供应商结算金额", "Initial accumulative supplier settlement amount", "val_1A", 1715291.2)
            val_1B = num_input("本期供应商结算（不含税）", "Current period supplier settlement (w/o VAT)", "val_1B", 0.0)
            val_1C = num_input("本期合同项下其他应付", "Current period other-payables under contract", "val_1C", 0.0)
        with col2:
            val_1D = num_input("本期合同项下其他扣款", "Current period other-deductions under contract", "val_1D", 0.0)
            
            st.markdown(f'<div class="cn-label">增值税率</div><div class="en-label">Current period VAT Rate</div>', unsafe_allow_html=True)
            vat_options = [0, 14, 9, 5, 10]
            vat_val = get_val("vat_rate", 14)
            vat_idx = vat_options.index(vat_val) if vat_val in vat_options else 1
            vat_rate = st.selectbox("hidden", vat_options, index=vat_idx, key="s_vat", label_visibility="collapsed")
            save_val("vat_rate", vat_rate)
            st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
            
            val_1E = round(val_1B * vat_rate / 100, 2)
            calc_display("本期增值税额", "Current period VAT amount", val_1E)
            
        val_1F = val_1A + val_1B + val_1C - val_1D + val_1E
        st.markdown('<br/>', unsafe_allow_html=True)
        calc_display("期末累计供应商结算金额", "Ending accumulative supplier settlement amount", val_1F)

    # 2. ADVANCE PAYMENT
    with st.container(border=True):
        st.markdown('<div class="section-title">预付款 | Advance Payment</div>', unsafe_allow_html=True)
        col1, col2 = st.columns(2)
        with col1:
            val_2A = num_input("合同项下预付款", "Advance payment under contract", "val_2A", 277500.0)
            val_2B = num_input("期初预付款扣除金额", "Initial deduction from advance payment", "val_2B", 256358.13)
        with col2:
            val_2C = num_input("本期预付款扣除金额", "Current period deduction from advance payment", "val_2C", 935.25)
            val_2D = val_2B + val_2C
            val_2E = val_2A - val_2D
            calc_display("期末预付款扣除金额", "Ending deduction from advance payment", val_2D)
            calc_display("期末预付款余额", "Ending balance of advance payment", val_2E)

    # 3. ENDING ACCUMULATIVE AMOUNT PAYABLE
    with st.container(border=True):
        val_3A = val_1F - val_2D
        st.markdown('<div class="section-title">累计应付金额 | Ending Accumulative Payable</div>', unsafe_allow_html=True)
        calc_display("期末累计应付金额", "Ending accumulative amount payable", val_3A)

    # DEDUCTIONS
    with st.container(border=True):
        st.markdown('<div class="section-title">扣款项目 | Deductions</div>', unsafe_allow_html=True)
        colA, colB = st.columns(2)
        with colA:
            # 4. RETENTION MONEY
            st.markdown('<div style="background:var(--text-color); color:var(--background-color); padding:4px 8px; font-weight:700; font-size:12px; margin-bottom:10px;">保留金 | RETENTION MONEY</div>', unsafe_allow_html=True)
            val_4A = num_input("期初保留金余额", "Initial balance of retention amount", "val_4A", 0.0)
            
            st.markdown(f'<div class="cn-label">扣除比例</div><div class="en-label">Rate</div>', unsafe_allow_html=True)
            ret_options = [0, 3, 5, 10]
            ret_val = get_val("ret_rate", 0)
            ret_idx = ret_options.index(ret_val) if ret_val in ret_options else 0
            retention_rate = st.selectbox("Rate", ret_options, index=ret_idx, key="s_ret", label_visibility="collapsed")
            save_val("ret_rate", retention_rate)
            st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
            
            val_4B = round(val_1B * retention_rate / 100, 2)
            calc_display("本期应扣保留金", "Current period retention money to be deducted", val_4B)
            
            val_4C = num_input("本期退回保留金", "Current period return of retention money", "val_4C", 0.0)
            val_4D = val_4A + val_4B - val_4C
            calc_display("期末保留金余额", "Ending balance of retention money", val_4D)

            # 5. TEMP LABOUR
            st.markdown('<br/>', unsafe_allow_html=True)
            st.markdown('<div style="background:var(--text-color); color:var(--background-color); padding:4px 8px; font-weight:700; font-size:12px; margin-bottom:10px;">临时劳务费社保 | TEMP LABOUR SOCIAL</div>', unsafe_allow_html=True)
            val_5A = num_input("期初临时劳务费社保余额", "Initial balance of temporary labour social insurance", "val_5A", 0.0)
            
            st.markdown(f'<div class="cn-label">扣除比例</div><div class="en-label">Rate</div>', unsafe_allow_html=True)
            temp_options = [0, 0.45]
            temp_val = get_val("temp_rate", 0)
            temp_idx = temp_options.index(temp_val) if temp_val in temp_options else 0
            temp_rate = st.selectbox("Rate", temp_options, index=temp_idx, key="s_temp", label_visibility="collapsed")
            save_val("temp_rate", temp_rate)
            st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
            
            val_5B = round(val_1B * temp_rate / 100, 2)
            calc_display("本期应扣临时劳务费", "Current temporary labour to deduct", val_5B)
            
            val_5C = num_input("本期退回临时劳务费", "Current return of temporary labour social insurance", "val_5C", 0.0)
            val_5D = val_5A + val_5B - val_5C
            calc_display("期末临时劳务费社保余额", "Ending balance of temporary labour social insurance", val_5D)

        with colB:
            # 6. WHT
            st.markdown('<div style="background:var(--text-color); color:var(--background-color); padding:4px 8px; font-weight:700; font-size:12px; margin-bottom:10px;">预提税 | WITHHOLDING TAX</div>', unsafe_allow_html=True)
            val_6A = num_input("期初累计预提税额", "Initial accumulative WHT", "val_6A", 0.0)
            
            st.markdown(f'<div class="cn-label">预提税率</div><div class="en-label">Rate</div>', unsafe_allow_html=True)
            wht_options = [0, 1, 3, 5]
            wht_val = get_val("wht_rate", 0)
            wht_idx = wht_options.index(wht_val) if wht_val in wht_options else 0
            wht_rate = st.selectbox("Rate", wht_options, index=wht_idx, key="s_wht", label_visibility="collapsed")
            save_val("wht_rate", wht_rate)
            st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
            
            val_6B = round(val_1B * wht_rate / 100, 2)
            calc_display("本期预提税", "Current period WHT", val_6B)
            val_6C = val_6A + val_6B
            calc_display("期末累计预提税额", "Ending accumulative WHT", val_6C)
            
            calc_display("应税金额 - 预提税", "Taxable - WHT", val_1B - val_6B)

            # 8. OTHER DEDUCTIONS
            st.markdown('<br/>', unsafe_allow_html=True)
            st.markdown('<div style="background:var(--text-color); color:var(--background-color); padding:4px 8px; font-weight:700; font-size:12px; margin-bottom:10px;">其他扣款 | OTHER DEDUCTIONS</div>', unsafe_allow_html=True)
            val_8A = num_input("期初其他扣款", "Initial other-deductions", "val_8A", 0.0)
            
            st.markdown(f'<div class="cn-label">扣除费率</div><div class="en-label">Rate (Other & Social bundle)</div>', unsafe_allow_html=True)
            oth_options = [0, 1, 2, 3, 5]
            oth_val = get_val("oth_rate", 0)
            oth_idx = oth_options.index(oth_val) if oth_val in oth_options else 0
            oth_rate = st.selectbox("Rate", oth_options, index=oth_idx, key="s_oth", label_visibility="collapsed")
            save_val("oth_rate", oth_rate)
            st.markdown('<div style="margin-bottom:8px;"></div>', unsafe_allow_html=True)
            
            val_8B = round(val_1B * oth_rate / 100, 2)
            calc_display("本期其他扣款", "Current other-deductions", val_8B)
            val_8C = val_8A + val_8B
            calc_display("期末累计其他扣款", "Ending accumulative other-deductions", val_8C)
            
        total_deductions = val_4B + val_5B + val_6B + val_8B
        st.markdown(f"""
        <div style="background:var(--background-color); padding:10px; border:2px solid var(--text-color); margin-top:20px; display:flex; justify-content:space-between; align-items:center;">
            <div>
                <div class="cn-label">扣款合计</div>
                <div class="en-label">Total Deductions</div>
            </div>
            <div style="font-family:monospace; font-size:16px; font-weight:800; color:var(--text-color);">EGP {total_deductions:,.2f}</div>
        </div>
        """, unsafe_allow_html=True)
            
    # 7. ACCUMULATIVE PAID
    with st.container(border=True):
        st.markdown('<div class="section-title">累计已付 | Accumulative Paid</div>', unsafe_allow_html=True)
        col1, col2 = st.columns(2)
        with col1:
            val_7A = num_input("期初累计实际支付金额", "Initial accumulative reality amount paid", "val_7A", 1302933.73)
        with col2:
            val_7B = val_7A + val_6A
            calc_display("期初累计总支付金额", "Initial total accumulative paid amount", val_7B)


    # 10 & 11. ENDING PAID
    with st.container(border=True):
        st.markdown('<div class="section-title">实际与期末合计 | Realities & Ending Totals</div>', unsafe_allow_html=True)
        col1, col2, col3 = st.columns(3)
        with col1:
            val_10A = (val_1B + val_1E) - val_2C - total_deductions
            calc_display("本期实际支付金额", "Current period reality amount paid", val_10A)
        with col2:
            val_11A = val_7A + val_10A + val_2A
            calc_display("期末累计实际支付金额", "Ending accumulative reality amount paid", val_11A)
        with col3:
            val_11B = val_11A + val_6C
            calc_display("期末累计总支付金额", "Ending total accumulative amount paid", val_11B)

    st.markdown('<div style="margin-top: 25px;"></div>', unsafe_allow_html=True)
    
    col_space, col_save, col_exp = st.columns([3, 1, 1])
    with col_save:
        if st.button("Save Current", type="secondary", use_container_width=True):
            save_config(st.session_state.config)
            st.success("Saved!")
    with col_exp:
        data = {
            "1A": val_1A, "1B": val_1B, "1C": val_1C, "1D": val_1D, "1E": val_1E, "1F": val_1F,
            "2A": val_2A, "2B": val_2B, "2C": val_2C, "2D": val_2D, "2E": val_2E,
            "3A": val_3A,
            "4A": val_4A, "4B": val_4B, "4C": val_4C, "4D": val_4D,
            "5A": val_5A, "5B": val_5B, "5C": val_5C, "5D": val_5D,
            "6A": val_6A, "6B": val_6B, "6C": val_6C,
            "7A": val_7A, "7B": val_7B,
            "8A": val_8A, "8B": val_8B, "8C": val_8C,
            "10A": val_10A,
            "11A": val_11A, "11B": val_11B
        }
        
        excel_data = export_to_excel(data)
        st.download_button(
            label="Export to Excel",
            data=excel_data,
            file_name=f"CSCEC_Settlement_{datetime.now().strftime('%Y%m%d_%H%M%S')}.xlsx",
            mime="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            type="primary",
            use_container_width=True
        )
    
    save_config(st.session_state.config)

if __name__ == "__main__":
    main()
