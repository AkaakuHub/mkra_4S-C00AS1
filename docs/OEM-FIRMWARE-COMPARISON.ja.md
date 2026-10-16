# 同系3社4Kチューナー 更新データの静的比較

公式配布物のコンテナー構造と共通領域を比較します。

## 公式取得元

- SHARP: https://jp.sharp/restricted/support/bd/usb_ud_data/HD30_BS1909122.zip
- MASPRO: https://www.maspro.co.jp/doc/sft/dt814/BS1909122.zip
- I-O DATA: https://lib.iodata.jp/lib/soft/h/hvt4kbc134.exe

I-O DATAの公式ライブラリ https://www.iodata.jp/lib/product/h/6051.htm が参照する
product.php?code=6051 → soft key 2171 → soft.php?code=2171 の情報から、
soft.jsのcreateDlPathに従って公式ダウンロード先を解決した。

完全なEXEは74,017,792 bytes、SHA256
`735426542d38284e57801ee0b8b1d4c0655a1fd2c7ae17406848df58d8480299`。
CABはoffset634234、宣言サイズ73377439、終端74011673。
PE証明書はoffset74011680、6112 bytesで、終端は実ファイル長と一致。
署名の暗号学的検証を完了したとの主張ではない。

7zで実行せず展開し、RVPの宣言長73,340,256 bytesと一致を確認。
CABには次巻を自身の名前で参照するflag2があり、7zは巻参照の警告を出す。
展開ファイルに対するデータエラー表示は無かったが、展開コマンド全体の無警告成功とは扱わない。

## 3社すべてで同じ領域を確認

| 項目 | SHARP | MASPRO | I-O DATA |
|---|---:|---:|---:|
| RVP bytes | 71,190,400 | 78,045,664 | 73,340,256 |
| 共通主要領域 [start,end) | [260,67490336) | 同左 | 同左 |
| 主要領域長 | 67,490,076 | 同左 | 同左 |
| メーカー別領域長 | 3,565,072 | 10,420,336 | 5,714,928 |
| 共通末尾の開始offset | 71,055,408 | 77,910,672 | 73,205,264 |
| 共通末尾長 | 134,992 | 同左 | 同左 |
| 共通2領域の割合 | 94.9918% | 86.6481% | 92.2073% |

- 主要領域 SHA256: `3648881fc9de1d0f61b2abeb8e5064cf7336e2d9048be40fc998834febde91bc`
- 共通末尾 SHA256: `f276f62d529e7a116dde4a1aacd5f51846cbf1cd3a56617bbcf1e77c26e6fa58`
- I-O DATA RVP SHA256: `bbf8b09d6eb69930d1ba61bbada6736db88ff86be4bff74dc8f5527c5788ed73`
- EUC-JPの全角ＢＳ１９０９１２２は3社ともoffset272。

共通主要領域は全バイト比較、末尾はサイズの差によるoffsetシフトを考慮して全バイト比較した。
可読ヘッダーも存在するため、RVP全体を暗号化データと断定するのは適切ではない。

## 分かっていないこと

共通基盤の存在は強く示されるが、共通部分がtuner/カーネル/ドライバのどれかは未特定。
メーカー別領域から実行コード・TS設定の差を取り出したわけではない。
従って、同じTS設定で動く・メーカー別ファームで2Kが有効になるとは断定しない。

## 再現と公開範囲

公式配布物を自分で取得し、EXEを起動せず7z等で展開して、RVPの長さ・ハッシュを照合する。多巻警告を無視して全体の成功とみなさない。入力ファイルや抽出物は公開Gitへ追加しない。

```sh
python3 tools/compare_oem_firmware.py \
  --rvp SHARP=private/HD30_134.RVP \
  --rvp MASPRO=private/HD30M_134.RVP \
  --rvp IODATA=private/HD30IO_134.RVP \
  --exe private/hvt4kbc134.exe --out private/firmware-comparison.json
```

比較ツールは入力ファイルを変更しません。純正ファイルと抽出物は同梱しません。
