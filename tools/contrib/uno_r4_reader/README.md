# Arduino UNO R4 MinimaでSPI ROMを読み出す

4S-C00AS1のIC8602を2回読み出して、2MiBのファイルをそれぞれ保存し、SHA-256を比較する。取得したJEDEC IDは`EF 70 15`。Arduino Coreは`renesas_uno 1.6.0`。この構成では全容量の読み出し成功はまだ確認できていない。

スケッチが扱う命令はJEDEC ID取得と通常読み出しだけで、ROMの書き込み・消去命令は含まない。UNO R4 Minimaの端子は5Vで動作するため、CS・MOSI・CLKはLOW出力と入力状態を切り替え、外部抵抗で3.3Vへ引き上げる。DOはA0のADCで読む。

## 配線

作業中はAS1本体のAC電源を外す。Arduinoの5V・VINには接続しない。抵抗は信号と3.3Vの間に接続する。

| ROMピン | 信号 | Arduino | ブレッドボードの接続 | 抵抗 |
|---|---|---|---|---|
| 1 | CS | D8 | J21–H21 | 4.7kΩで3.3Vへ |
| 2 | DO | A0 | I22–F22 | なし |
| 3 | IO2 / WP | 未接続 | H23 | なし |
| 4 | GND | GND | G24–F24 | なし |
| 5 | DI | D9 | D42–E42 | 4.7kΩで3.3Vへ |
| 6 | CLK | D7 | C41–E41 | 4.7kΩで3.3Vへ |
| 7 | IO3 / HOLD | 未接続 | B40 | なし |
| 8 | VCC | 3.3V | A39–E39 | なし |

Arduinoの3.3V、E39、抵抗末端のJ13・D47・B48は共通の電源レールに接続する。3番・7番のクリップ線は取り外されている。この表は現在の実機配線の記録であり、読み出し成功を確認した配線ではない。3番・7番の機能と処置はチップの型番・設定に従って判断する。

## 実行

Arduino IDEで`uno_r4_reader.ino`を開き、UNO R4 MinimaとCOM3を選んでアップロードする。シリアルモニターを閉じ、リポジトリのルートで実行する。

```powershell
python -m pip install pyserial
python tools/contrib/uno_r4_reader/read_as1_windows.py COM3 --probe
python tools/contrib/uno_r4_reader/read_as1_windows.py COM3
```

最初にIDを5回確認し、0x000000・0x010000・0x1FF000をそれぞれ2回読む。不一致があれば差分を最大16バイト表示して停止する。再試行や多数決で不一致を隠す処理はない。

試験に通れば、作業ディレクトリに`as1-original-1.bin`と`as1-original-2.bin`を保存する。全容量のSHA-256が一致し、全00・全FFでない場合だけ成功と表示する。既存ファイルは上書きしない。通信中断時のファイルは不完全なため、バックアップとして使わない。ROMデータはGitに含めない。

## 現在の結果

ROMの8番–4番の電圧は利用者の測定で3.178V。COM3でIDの`EF 70 15`を確認したが、0x010000の256バイト中213バイトが2回の読み出しで異なった。例は`30→18`、`75→3A`、`08→84`。連続する値に1ビットのずれが見られる。原因は未確定で、バックアップは未取得。

## mkra導入との関係

本体へのroot telnetを用意する前に、自分のROMを保存する。[導入手順](../../../docs/SETUP.md)と[パッチツール](../patch_bootargs.py)を参照する。パッチツールは取得済みのファイルを変更するもので、ROMを読み出す機能はない。

参考: [UNO R4 Minimaの公式資料](https://docs.arduino.cc/hardware/uno-r4-minima/)、[WinbondのW25Q16JV資料一覧](https://www.winbond.com/hq/support/documentation/?__locale=en&category=%2F.categories%2Fresources%2Fdatasheet%2F&family=%2Fproduct%2Fcode-storage-flash-memory%2Fserial-nor-flash%2Findex.html&line=%2Fproduct%2Fcode-storage-flash-memory%2Findex.html&pno=W25Q16JV)。
