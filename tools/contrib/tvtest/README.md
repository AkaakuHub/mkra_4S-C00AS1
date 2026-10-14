# Windows版TVTestでBS4Kを視聴する

TVTestの64bit版と[BonDriver_Mirakurun v2.0](https://github.com/Chinachu/BonDriver_Mirakurun/releases/tag/v2.0)を同じフォルダーに置く。映像デコーダーには[LAV Filtersの64bit版](https://github.com/Nevcairiel/LAVFilters/releases)を使用する。

LAV FiltersのZIPを任意の保存用フォルダーへ展開し、次のコマンドでLAV Video Decoderを現在のWindowsユーザーに登録する。登録したデコーダーはこの展開先を使うため、実行後に移動・削除しない。

```powershell
python tools/contrib/tvtest/register_lav_video_windows.py "LAV Filtersの展開先"
```

スクリプトは`RegOverridePredefKey`で登録先を現在のWindowsユーザーの`Software\Classes`に変更し、LAV Video Decoderの`DllRegisterServer`を実行する。LAV Audio DecoderとLAV Splitterは登録しない。

`BonDriver_Mirakurun.ini`を次のように設定する。

```ini
[GLOBAL]
SERVER_HOST="本体のIPアドレス"
SERVER_PORT=40772
DECODE_B25=0
PRIORITY=0
SERVICE_SPLIT=0
```

TVTestの設定でH.265/HEVCデコーダーを`LAV Video Decoder`にする。BonDriverは`BonDriver_Mirakurun.dll`を選び、BS4Kのチャンネルを選択する。v2.0はAPIからチャンネル種別を取得するので、`CHANNEL_TYPES`の指定は不要。

本体がスクランブルを解除したHEVC映像とAAC音声をmkraが配信し、PC側で映像・音声をデコードする。このリポジトリには[音声対応の修正](https://github.com/Yocchan1513/mkra_4S-C00AS1/commit/2c0924727f8a12bf68ed9102913e149da5e5651f)を取り込み、TVTestが扱えるAAC-ADTS形式で配信する。AAC音声の再圧縮は行わず、LATMの格納形式をADTSへ変更する。

## 取得データの検証

2026年10月14日に、WindowsからNHK BSP4Kを取得した。最初の10秒間で31,337,156バイトを取得し、本体とPCで計算したMD5が一致した。166,687パケットのTS同期率は100%、CCエラーは0だった。HEVC映像617フレームとAAC-LATM音声443フレームをデコードできた。

続いて12秒間のHTTPライブ配信から35,666,984バイトを取得し、189,718パケットの同期率100%、CCエラー0を確認した。映像は3840×2160、音声は48kHz・2chだった。取得の開始部分にはHEVCの参照情報不足があり、12秒で接続を切ったファイルの末尾では不完全な音声パケットを検出した。

音声対応版を転送する前にも、TVTestの画面で3840×2160の映像表示とD・E・Sカウンターがすべて0であることを確認した。標準のTVTestはAAC-LATMを扱わないため、この時点では無音だった。
