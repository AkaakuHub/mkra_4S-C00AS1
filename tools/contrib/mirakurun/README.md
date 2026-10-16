# 本体のUbuntu環境でMirakurunを動かす

2026年10月17日に、純正のLinux 3.10.24を残したまま、RAM上のUbuntuへchrootしてMirakurun本体を実行しました。PCから`/api/version`と`/api/status`へ接続し、HTTP 200とバージョン4.1.5を確認しています。BS/CSの2Kを取得する受信プログラムとの連携は未完成です。検証時のチューナーとチャンネルの設定は空配列です。

## 確認した構成

| ソフトウェア | バージョン |
|---|---|
| Ubuntu Base armhf | 20.04.5 |
| Node.js linux-armv7l | 22.23.3 |
| npm | 10.9.9 |
| Mirakurun | 4.1.5 |
| Ubuntuのlibatomic1 | 10.5.0-1ubuntu1~20.04 |
| ARM用の`@node-rs/crc32-linux-arm-gnueabihf` | 1.4.3 |

UbuntuとNode.jsは公式アーカイブから取得し、配布元のSHA-256と照合しました。

| ファイル | SHA-256 |
|---|---|
| `ubuntu-base-20.04.5-base-armhf.tar.gz` | `6bcbfa7f603d79d368d40e138dad98938907d2fb0d6416521417cf8702c2f5de` |
| `node-v22.23.3-linux-armv7l.tar.xz` | `590a2768199bdd0b848648a81cd7f07f88a5ceb27949f0c70d5e6652c3eaca69` |

配布元は[Ubuntu Base](https://cdimage.ubuntu.com/ubuntu-base/releases/20.04/release/)と[Node.js](https://nodejs.org/dist/v22.23.3/)です。Mirakurunと依存パッケージはnpmの配布物を使いました。Windowsで準備した依存パッケージにはWindows用のCRC32モジュールが含まれるため、ARM Linux用のモジュールを別途配置しています。実機で既知の入力に対するCRC32の計算を確認しました。

## 配置と起動

Ubuntuのルートを`/tmp/mt/ubuntu`に展開し、次のファイルをその内側に配置します。`/tmp/mt`はRAM上にあり、本体を再起動すると消えます。ROMやeMMCへのインストールは行っていません。

```text
/usr/local/bin/node
/usr/local/bin/npm
/usr/local/lib/node_modules/npm/
/opt/mirakurun/node_modules/mirakurun/
/opt/mirakurun/node_modules/（Mirakurunの依存パッケージ）
/usr/local/etc/mirakurun/server.yml
/usr/local/etc/mirakurun/tuners.yml
/usr/local/etc/mirakurun/channels.yml
```

Node.jsには`libatomic.so.1`が必要でした。Ubuntuの`libatomic1`パッケージをchroot内へ展開しています。また、通常のNode.js実行には`/dev/random`と`/dev/urandom`が必要でした。`node --version`だけが成功しても、通常の実行を確認したことにはなりません。

本体側で、chrootのデバイス、DNS、読み取り専用のprocマウントを用意します。

```sh
cp -a /dev/null /dev/random /dev/urandom /tmp/mt/ubuntu/dev/
cp /tmp/resolv.conf /tmp/mt/ubuntu/etc/resolv.conf
mount --bind /proc /tmp/mt/ubuntu/proc
mount -o remount,bind,ro /tmp/mt/ubuntu/proc
```

`/proc`がないと`/api/status`のメモリ使用量取得が失敗します。`npm`と、その子プロセスから`node`を検索できるPATHも必要です。`/api/version`は内部で`npm view mirakurun version`を実行するため、DNSと外部のnpmレジストリへの接続も使用します。

検証した`server.yml`は次の内容です。`tuners.yml`と`channels.yml`は、それぞれ`[]`を記述しています。

```yaml
port: 40772
disableIPv6: true
logLevel: 2
maxLogHistory: 50
jobMaxRunning: 1
jobMaxStandby: 1
allowIPv4CidrRanges:
  - 127.0.0.0/8
  - 192.168.1.0/24
```

同じディレクトリの`start_chroot.sh`を本体の`/tmp/mt/start-mirakurun`へ転送し、実行権限を付けて起動します。起動スクリプトはフォアグラウンドで動作します。

```sh
/tmp/mt/start-mirakurun > /tmp/mt/mirakurun.log 2>&1 &
```

検証時の本体IPは`192.168.1.27`でした。PCから次のAPIを確認しました。

```text
http://192.168.1.27:40772/api/version
http://192.168.1.27:40772/api/status
http://192.168.1.27:40772/api/tuners
```

`/api/tuners`は空配列です。これらの応答はMirakurun本体とネットワークの動作確認であり、2Kの受信成功を示すものではありません。受信側の実測は調査記録にあります。

## mkraとの接続（2026年10月18日）

Mirakurun 4.1.5のチューナーコマンドに`mkra remux4k --out -`を設定しました。Mirakurunがmkraの標準出力からMPEG-TSを受け取る構成です。mkraの診断ログは標準エラー出力へ移し、TSへ混ざらないようにしました。チャンネルは`BS4K/BS17_0`、APIのポートは、既存のmkraサーバーと同時に確認するため`40773`です。

`build_overlay.py`は保存済みのMirakurun環境、Node.js、libatomic、ARM版mkraから追加ファイルのアーカイブを作ります。必要な引数は、保存済みアーカイブのディレクトリ、ARM版mkra、出力先、作業ディレクトリの順です。Ubuntu Baseと`mirakurun-runtime.tar.gz`を先に展開し、追加アーカイブを同じルートへ重ねます。Node.jsの配布アーカイブ全体ではなく、実行ファイルだけを配置します。

chroot内には`/dev/null`、`/dev/random`、`/dev/urandom`、`/dev/kmem`、`/dev/vixs/xcodedrv`を用意し、純正の`/proc`と`/tmp/adsb`をbindマウントします。`start_chroot.sh`が必要なデバイスとファイルを起動前に検査します。全体で本体の`/tmp`を約246MiB使用したため、起動前後に`df -h /tmp`と`free -m`を確認してください。配置はRAM上だけで、再起動すると消えます。

Mirakurun本体にはBS4Kのチャンネル種別がないため、保存済みのランタイムの`Channel.js`、`common.js`、`ServiceItem.js`へBS4Kを追加しています。4Kのremuxは元のTSにSDTを含まないため、`Service.js`のBS4K自動サービス探索を省きます。受信コマンドがエラー終了したときにMirakurunが約0.1秒ごとに再起動していたため、`TunerDevice.js`でエラー時の再起動を止めています。BS/CS2Kの自動探索処理は変更していません。

実機ではPCから`http://192.168.1.27:40773/`、`/api/status`、`/api/tuners`、`/api/channels`がHTTP 200で応答しました。Web画面のHTML、BS4Kチューナー1台、BS4Kチャンネル1件を確認しました。一方、その時点では製品の4K映像・音声ESリングが存在せず、ストリームAPIはHTTP 200の後に0バイトで終了しました。TSをPCへ送れたという確認ではありません。Mirakurunを動かしても2KのTS取得経路は作られないため、BS/CS2Kの受信コマンドは引き続き調査が必要です。
