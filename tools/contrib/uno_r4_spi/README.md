# Arduino UNO R4 MinimaでSPI ROMを読み出してtelnetを有効化する

4S-C00AS1のIC8602を2回読み出し、それぞれ2MiBのファイルとして保存する。2本のSHA-256を比較し、内容の一致を確認する。取得したJEDEC IDは`EF 70 15`で、Arduino Coreは`renesas_uno 1.6.0`を使用した。2026年10月14日、この配線で2,097,152バイトの2回取得とSHA-256一致を確認した。

読み出し・書き込みのコードは、利用者が用意したUNO R4用コードを基にしている。telnet有効化の環境データは、このリポジトリ付属の`patch_bootargs.py`をそのまま使う。

Pythonの読み出しツールはID取得・設定値取得・通常読み出しを送る。Arduinoスケッチには、最終4KiBに限定した消去・ページ書き込みも含まれる。UNO R4 Minimaの端子は5Vで動作するため、CS・MOSI・CLKはNMOSオープンドレインに設定してLOWとHi-Zを切り替え、外部抵抗で3.3Vへ引き上げる。DOはA0のADCで読む。

NMOS設定には[RenesasのI/OポートAPI](https://renesas.github.io/fsp/group___i_o_p_o_r_t.html)を使う。Core 1.6.0の`OUTPUT_OPENDRAIN`はPMOSを選ぶ実装のため使用しない。[Arduino Coreの報告](https://github.com/arduino/ArduinoCore-renesas/issues/343)も参照。

## 配線

作業中はAS1本体のAC電源を外す。Arduinoの5V・VINには接続しない。抵抗は信号と3.3Vの間に接続する。

| ROMピン | 信号 | Arduino | ブレッドボードの接続 | 抵抗 |
|---|---|---|---|---|
| 1 | CS | D8 | J21–H21 | 4.7kΩで3.3Vへ |
| 2 | DO | A0 | I22–F22 | なし |
| 3 | IO2 / WP | 3.3Vへプルアップ | H23–F23 | F23–F13に4.7kΩ |
| 4 | GND | GND | G24–F24 | なし |
| 5 | DI | D9 | D42–E42 | 4.7kΩで3.3Vへ |
| 6 | CLK | D7 | C41–E41 | 4.7kΩを2本並列で3.3Vへ |
| 7 | IO3 / HOLD | 3.3Vへプルアップ | B40–E40 | E40–E48に4.7kΩ |
| 8 | VCC | 3.3V | A39–E39 | なし |

Arduinoの3.3V、E39、抵抗末端のJ13・D47・B48は共通の電源レールに接続する。3番・7番のクリップ線も接続する。CLKに追加した抵抗はD41–D48間に接続し、既存の4.7kΩと並列にする。D48の代わりに同じ列のC48・A48も使える。抵抗は計6本で、CLKの合成抵抗は約2.35kΩ。この表は、全容量を2回読み出して一致した実機の配線を記録している。

その後、設定値を5回読み出して`SR1=00・SR2=00・SR3=60`を確認した。QE=0なので、7番のHOLD機能は有効になっている。追加の4.7kΩ抵抗をE40–E48間に接続し、7番のクリップ線を戻した。E40はB40と同じ接続列、E48は3.3VにつながるB48と同じ接続列。配線変更時はArduinoのUSBを外す。[Winbondのデータシート](https://wiki.amperka.ru/_media/products%3Araspberry-pi-pico%3Aw25q16jvuxiq-datasheet.pdf)では、HOLDがLOWになるとDOが高インピーダンスになり、CLKとDIが無視されると説明されている。

## 実行

Arduino IDEで`uno_r4_spi.ino`を開き、UNO R4 MinimaとCOM3を選んでアップロードする。シリアルモニターを閉じ、リポジトリのルートで実行する。読み出し処理中にアップロードしない。

```powershell
python -m pip install pyserial
python tools/contrib/uno_r4_spi/read_as1_windows.py COM3 --probe
python tools/contrib/uno_r4_spi/read_as1_windows.py COM3
```

最初にIDを5回確認し、0x000000・0x010000・0x1FF000をそれぞれ2回読む。不一致があれば差分を最大16バイト表示して停止する。再試行や多数決で不一致を隠す処理はない。

設定値を表示し、環境データの4KiB全体も2回取得して一致とCRCを確認する。短い試験読み出しが通っても、環境データに不一致があれば全容量の取得を開始しない。

試験に通れば、作業ディレクトリに`as1-original-1.bin`と`as1-original-2.bin`を保存する。全容量のSHA-256が一致し、全00・全FFでない場合だけ成功と表示する。既存ファイルは上書きしない。通信中断時のファイルは不完全なため、バックアップとして使わない。ROMデータはGitに含めない。

5秒ごとに読み出し回数、進捗バー、割合、取得量、速度、残り時間を表示する。ターミナルでは同じ行を更新し、ログへの出力では各更新を別の行に残す。

## telnetを有効化する

2回のダンプが一致してから実行する。書き込み前にバックアップを別の場所にも保管する。

```powershell
python tools/contrib/patch_bootargs.py as1-original-1.bin -o telnet.bin
python -u tools/contrib/uno_r4_spi/write_telnet_sector_windows.py COM3 as1-original-1.bin telnet.bin --yes-write
```

`--yes-write`は消去・書き込みの実行指定であり、追加の入力待ちはない。書き込みツールは2本のバックアップの一致、サイズ、元と変更後の環境CRC、標準の`set_bootargs`、付属ツールのtelnetパッチとの完全一致を確認する。ROM現物の最終4KiBがバックアップと一致した場合だけ、最終セクターを消去して16ページを書き込む。

各ページを読み戻した後、ROM全体を読み戻して`telnet.bin`と比較する。検証に失敗した場合はAS1を起動しない。全容量の検証が終わってからArduinoのUSB電源を外し、ROMクリップを外す。その後、本体を組み立ててLANを接続し、起動後にTCPポート2323へのtelnet接続を確認する。パスワードなしのrootシェルになるため、隔離したLANで確認する。これが確認できれば通常のmkra導入にArduinoは不要になる。

## CLKの測定

`measure_clock_windows.py`はスケッチの`C`・`B`命令を使い、CLKのADC値を表示する。ArduinoのUSBを外してから、A1をCLKと同じ接続列につなぐ。上の配線ではD41を追加抵抗が使うため、同じ接続列の空いた穴を使う。通常の読み出し・書き込みでは測定用ジャンパー線を外す。

```powershell
python tools/contrib/uno_r4_spi/measure_clock_windows.py COM3 --address 0x1FF000 --attempts 5
python tools/contrib/uno_r4_spi/measure_clock_windows.py COM3 --idle --attempts 5
```

通常測定では256バイトのSHA-256、初回との相違バイト数、LOW・HIGHそれぞれのADC最小値・最大値を表示する。`--idle`ではCLKをHi-Zにした状態のADC最小値・最大値を表示する。測定はSPIの各ビット処理にADC変換を追加するため、通常の読み出しとはタイミングが変わる。短いパルスを検出できないため、この結果だけで信号の正常性は判断できない。

LAN上のIPアドレスが不明な場合は、[Windows用の機器探索ツール](../network/README.md)を参照する。

## 現在の結果

初期の配線では、利用者が測定したROMの8番–4番の電圧は3.178Vだった。COM3でIDの`EF 70 15`を確認したが、0x010000の256バイト中213バイトが2回の読み出しで異なった。例は`30→18`、`75→3A`、`08→84`。連続する値に1ビットのずれが見られた。

最初の2MiBダンプは環境CRCが一致しなかった。2回目も取得済み168KiBで7344バイトが異なったため中止した。HOLDの抵抗を追加した後、利用者が測定した7番–4番の電圧は3.1Vだった。NMOS制御でも読み出し不一致が残った。DOのADC値はLOWが0–9、HIGHが688–700で、しきい値410付近の値は観測されなかった。

同じクロック位置でADCを連続2回測ると判定は一致したが、環境データの読み出しは不一致だった。さらに[Winbondの揮発性ステータス設定仕様](https://www.kstmicro.com/wp-content/uploads/2024/07/w25q16jv-spi-revg-03222018-plus.pdf)に従ったHOLDの一時無効化、出力駆動能力100%、各クロック段階で50µs待つ試験も行ったが、環境データの一致には至らなかった。この時点では原因を確定できず、CLK信号線とクリップの接触を含む実機配線を次の調査対象とした。

CLKをArduinoのA1でも測定すると、LOWはADC値0–7、HIGHは674–688だった。こちらからクロックを出さない状態では、測定範囲でLOWは観測されなかった。ADCでは短い余分なパルスを見逃すため、これだけではCLKの正常性を保証できない。診断用の`C`命令は`R`と同じアドレスを受け取り、256バイトのデータに続けて各ビットのLOW・HIGHのADC値4096個を16ビットのリトルエンディアンで返す。`B`命令はCLKをHi-Zにして4096個のADC値を返す。この診断時だけD41–Arduino A1を接続する。通常の6本の抵抗による配線では、この測定用ジャンパー線を外す。

抵抗を6本にして配線を接続し直した後、IDの5回一致、3か所の試験読み出し、環境データ4KiBの2回一致とCRC一致を確認した。ROM全体の2回取得もSHA-256が一致した。取得速度は約2.7KiB/秒。以前の不一致の原因は未確定。

一致したバックアップから付属ツールでtelnet用データを作成し、最終4KiB内の222バイトだけが変わることを確認した。ROM現物の環境データもバックアップと一致してから消去・書き込みを行い、16ページすべての読み戻しが一致した。書き込み後の全2MiBも1バイトずつ照合して一致し、SHA-256もtelnet用データと一致した。本体を組み立てて起動した後、TCPポート2323へのtelnet接続に成功し、`id -u`が`0`を返すことと、`/proc/cmdline`にtelnet有効化の起動設定が反映されていることを確認した。通常のmkra導入にArduinoは不要になる。

## mkra導入との関係

本体へのroot telnetを用意する前に、自分のROMを保存する。[導入手順](../../../docs/SETUP.md)と[パッチツール](../patch_bootargs.py)を参照する。パッチツールは取得済みのファイルを変更するもので、ROMを読み出す機能はない。

参考: [UNO R4 Minimaの公式資料](https://docs.arduino.cc/hardware/uno-r4-minima/)、[WinbondのW25Q16JV資料一覧](https://www.winbond.com/hq/support/documentation/?__locale=en&category=%2F.categories%2Fresources%2Fdatasheet%2F&family=%2Fproduct%2Fcode-storage-flash-memory%2Fserial-nor-flash%2Findex.html&line=%2Fproduct%2Fcode-storage-flash-memory%2Findex.html&pno=W25Q16JV)。
