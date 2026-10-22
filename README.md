# mkra — SHARP 4S-C00AS1向けBS2K・BS4K受信

SHARP 4S-C00AS1をMirakurun互換のネットワークチューナーとして使う実験的なForkです。ARMv7向けの静的実行ファイルで動作し、受信機へのNode.jsやchrootの導入は不要です。

[otti83さんのmkra](https://github.com/otti83/mkra_4S-C00AS1)のBS4K解析・REMUX・HTTP APIを基にしています。BS2K受信・内蔵CAS復号・EPGStation連携は[Karin-LaboratoryのFork](https://github.com/Karin-Laboratory/mkra_4S-C00AS1)から取り込みました。両プロジェクトの作者の方々に感謝します。

## 機能

- 通常BSのISDB-S/MPEG-TS受信と内蔵CASを使ったライブMULTI2復号。
- BS4KのISDB-S3/MMTからMPEG-TSへの変換。
- BS2K・BS4K共通のMirakurun互換HTTP APIと、同じTSへの接続共有。
- 番組情報の収集、EPGStation連携、外部サーバーからのRAMへの再投入。
- 本体のfreeze待機、Magic Packetによる起床、録画後の自動再待機。

物理チューナーは1基です。異なるTSの同時受信はできません。2K/4Kの交互受信と本体待機からの復帰を実機確認しました。長時間運用とHDMI映像復帰は保証しません。本体待機の消費電力は未測定です。詳細は[制限事項](docs/LIMITATIONS.md)を参照してください。

## 必要な環境

- SHARP 4S-C00AS1、BSアンテナ、正常に受信できる内蔵CAS環境。
- 本体へのroot Telnet接続など、既存の管理手段。
- 稼働中の純正Linuxとtuner/card/abs/adsbサービス。
- ビルド用のRustとARMv7クロスコンパイル環境。

純正実行ファイルのハッシュと固定ABIを照合します。異なるファームウェアや類似機種では未検証です。本ForkはTelnet有効化やフラッシュ変更を自動実行しません。

## ビルドと起動

```sh
git clone https://github.com/AkaakuHub/mkra_4S-C00AS1.git
cd mkra_4S-C00AS1
rustup target add armv7-unknown-linux-musleabihf
./tools/build_unified.sh
```

生成した`target/armv7-unknown-linux-musleabihf/release/mkra`を本体の`/tmp/mt/mkra`へ転送し、実行権限を付けます。BS2Kの使用には[カーネルモジュール](tools/contrib/power/kernel/README.md)の適用が必要です。録画中の入れ替えや既存サーバーとの二重起動を避けてください。

```sh
/tmp/mt/mkra serve --remux4k --tune --bs2k --channel-type BS4K --port 40772
```

BS朝日以外の通常BS5局も使う場合は、`config/bs-additional-main.tsv`を転送し、`--isdbs-channels /tmp/mt/bs-additional-main.tsv`を追加します。本体待機を使う場合は[電源制御](tools/contrib/power/README.md)のモジュールが必要です。

```sh
curl http://198.51.100.10:40772/api/services
curl http://198.51.100.10:40772/api/services/400151/stream -o test.ts
```

`198.51.100.10`は受信機のアドレスへ置き換えてください。BS朝日2Kは`400151`、BS朝日4Kは`1100151`です。配置先はRAMなので、再起動後は再投入が必要です。

## ドキュメント

- [導入手順](docs/SETUP.md) / [API](docs/API.md) / [制限事項](docs/LIMITATIONS.md)
- [受信の仕組み](docs/HOW_IT_WORKS.md) / [BS2K受信経路](docs/BS2K-RECEPTION.ja.md) / [内蔵CAS](docs/BS2K-CAS-INVESTIGATION.ja.md)
- [ABI](docs/ABI.md) / [電源制御](tools/contrib/power/README.md)
- [EPGStation導入](docs/EPGSTATION-INSTALL.ja.md) / [EPGツール](tools/epgstation/README.ja.md)
- [RAMへの自動再投入](docs/MKRA-AUTO-RESTORE.ja.md) / [トラブル対応](docs/TROUBLESHOOTING.ja.md)

## ライセンス

[MIT LICENSE](LICENSE)の著作権表示を保持しています。MULTI2参照実装のライセンスは[NOTICE](tools/third-party/libarib25-NOTICE.md)を参照してください。純正ファームウェア、鍵、カード固有情報、放送録画は同梱しません。無保証です。
