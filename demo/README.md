# 互動展示版

公開網址：https://wkltueserc20.github.io/SICK-GLS611lector-monitor/

純靜態 HTML、CSS、JavaScript，無需 Rust、Python、Node.js 或真實設備。GitHub Pages 僅發布本目錄，不會發布照片、正式設定與執行紀錄。

## 可體驗功能

- 一台／四台／十台快速切換與自動排版。
- 每 2.5 秒模擬一台設備的讀碼，亦可指定設備手動模擬 OK／NoRead。
- 最新照片恢復 OK 後，歷史 NG 仍保留。
- 照片放大、旋轉、顯示篩選、條碼複製與暫停畫面。
- 設備名稱、IPv4、FTP 埠、啟用／停用、排序及設定匯入匯出。

右上角「設備設定」儲存後立即套用展示，設定只存於目前瀏覽器的 `chingtech-monitor-public-demo-v1` localStorage。正式版需重開程式套用。數量快捷鍵會將展示重設為該數量的預設資料；重新整理會重建模擬照片與計數。

「暫停畫面」只暫停顯示；「自動模擬：關」停止自動產生結果。模擬不代表真實傳輸效能、讀碼率或設備連線狀態。

## 資料與限制

所有圖片為程式繪製的 SVG 示意圖，點陣紋理不是有效條碼。條碼文字為假的 `DEMO` 編號，IP 使用文件示範位址。沒有現場照片、設備序號或實際設定。即使自行填入真實 IP，程式也不會向該位址發出請求。

不接收 FTP、不連線相機、不呼叫桌面程式 API、不保存實際照片、不模擬真正解碼。匯入 JSON 僅在本機瀏覽器解析，匯出透過瀏覽器下載；不會上傳到 GitHub。

## 開發與發布

`simulator.js` 與 `controls.js` 是展示版行為；共用介面從 `assets/` 產生。修改正式介面後執行：

```text
python scripts/build_demo.py
python -m http.server 8099 --bind 127.0.0.1 --directory demo
```

開啟 `http://127.0.0.1:8099/` 測試。不需要安裝任何前端套件。展示資源全部使用相對路徑，支援 GitHub Pages 的儲存庫子路徑。

GitHub 儲存庫 Settings → Pages 的來源選 GitHub Actions；推送 `demo/` 修改至 main，會執行 `.github/workflows/demo-pages.yml`。只部署 `demo/`，不部署整個儲存庫。
