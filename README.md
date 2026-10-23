# 擎添工業轉接平台影像監控 · Rust 2.1.1

Windows 10/11 x64 應用程式；沿用網頁畫面，FTP、HTTP、XML 判定、存檔、清理與視窗生命週期均由 Rust 執行，不需要 Python 或 Node.js。視窗使用系統 Microsoft Edge WebView2 Runtime。

原生執行環境採靜態連結，不需要另外安裝 Visual C++ Redistributable。這是待四台 SICK 實機驗收的遷移版本，尚未執行新電腦的安全政策與簽章驗收。

## 部署

1. 關閉 Python 版監控視窗，備份現有 `config.json`、`photos`、`ng_photos`、`data`。
2. 把 `ChingTech-Monitor-Rust.exe` 放進 `D:\轉接平台照片`，雙擊啟動。
3. 程式使用 EXE 所在資料夾儲存資料，可沿用原有 Python 版資料。首次啟動將重新掃描 XML 並整理既有 NG。
4. 四台相機 FTP 目的仍是這台電腦的 IPv4，埠 2121，使用者和密碼 `anonymous`。
5. 防火牆入站允許 TCP 2121、30000–30010，來源限制為四台相機 IP。網頁只綁定 127.0.0.1。
6. 四台各實際觸發一次 OK 和 NoRead，確認對應位置及最近 NG。
7. 關閉應用程式視窗即停止接收；已存照片不刪除。不能與 Python 版同時使用相同連接埠。

未簽章的 EXE 仍可能被 Windows Smart App Control 封鎖。正式部署需受信任的程式碼簽章或 IT 審核；Rust 不會繞過安全政策。

## 2.1 介面更新

2.1.1：加入相機＋四格監控專屬圖示，EXE、桌面捷徑與視窗均可使用。若舊捷徑仍顯示快取圖示，可在捷徑內容的「變更圖示」選取包內 ChingTech-Monitor.ico，或刪除舊捷徑後由新 EXE 建立。

- 固定四個位置排列，加入方位圖示、最新 NG 與歷史 NG 分開統計。
- 單一位置檢視，及「最新＋NG／只看最新／只看 NG」切換。
- 條碼內容加大並可複製，接收時間同時顯示距今多久。
- 照片可放大、旋轉；檢查時固定照片像素，不受新照片覆蓋影響。
- 暫停會明確提示背景仍持續接收。空白鍵可暫停／繼續。
- 服務無回應時保留畫面並自動重試，不把舊照片當成連線成功。
- 「說明」可查看實際資料位置與目前保留上限。

從 2.0 更新：先關閉監控視窗，以新版 EXE 取代舊 EXE；保留既有 config.json、photos、ng_photos、data。若桌面捷徑仍指向同一個 EXE 檔名，不需重建捷徑。

## 設定

`config.json` 首次啟動自動建立，舊版只有三個埠設定的檔案也相容。

```json
{
  "ftp_host": "0.0.0.0",
  "ftp_port": 2121,
  "web_port": 8080,
  "max_photos": 1000,
  "cameras": [
    {"ip":"192.168.12.17", "label":"左上"},
    {"ip":"192.168.12.18", "label":"右上"},
    {"ip":"192.168.12.19", "label":"左下"},
    {"ip":"192.168.12.20", "label":"右下"}
  ]
}
```

修改 IP 時，需同步更新相機本身的網路設定及 Windows 防火牆來源。四個項目的順序對應左上、右上、左下、右下。關閉程式後修改，再重開生效。

## 資料

- `photos/設備IP`：OK 或待確認的原始影像與 XML。
- `ng_photos/設備IP`：確認 NG 的照片與 XML。
- 每個 IP 兩個資料夾合計最多 `max_photos` 張，超出時依檔案時間刪除最舊照片與 XML。
- `data`：最新照片、最近 NG 快取與狀態，相容原版。
- `logs/receiver-rust.log`：接收錯誤紀錄，自動輪替。

解碼規則沿用現版：有效 `symbol/result` 是 OK；NoRead 或已處理但無條碼結果是 NG；缺少可判定的 XML 是待確認。這是照片解碼判定，並非 PLC 最終比對結果。

## 建置與測試

建置需要 Windows x64、Rust stable（MSVC 工具鏈）、Microsoft C++ Build Tools 與 Windows SDK 的資源編譯器。執行監控視窗需要 Microsoft Edge WebView2 Runtime。

```text
cargo test --locked
cargo build --release --locked
```

`Cargo.lock` 固定相依套件。介面與 LOGO 內嵌在 EXE。`--data-dir <資料夾>` 可指定另一個資料位置。`--headless` 用於接收服務自動測試；`--ui-smoke-test` 會開啟視窗並在四秒後關閉。開發測試請使用獨立資料夾和連接埠。

四路 FTP 整合測試為 `tests/system_test.py`，使用暫存資料與獨立連接埠，不會修改相機設定。先執行 release 建置，再在專案根目錄執行 `python tests/system_test.py`；也可用 `CT_RUST_EXE` 環境變數指定待測 EXE。Python 僅用於開發驗證，部署不需要 Python。

`config.example.json` 是設定範本；執行時的 `config.json`、生產照片、NG 照片、紀錄和建置輸出均排除於 Git 之外。

已通過：Rust 單元測試、四個來源 IP 同時與連續 FTP 傳送、主動／被動模式、照片與 XML 先後順序、同名檔案隔離、NG 歸檔與保留上限、16 張既有照片副本與 Python 判定比對、重開後狀態保存，以及原生視窗結束後 FTP／HTTP 端口釋放。
