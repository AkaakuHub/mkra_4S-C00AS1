# Changelog

## 配布設定 — 2026-10-17

- proxy/4K・2K scanner/timer/共通env/tmpfiles/EPGStation unit雛形を完備。
- 4K scannerがplaceholderのstoredEvents=0で初回収集を止める不具合を修正し、合成回帰試験を追加。両scannerのEPGStation URLを共通envから指定可能にした。
- CIではsetup-nodeの実パスを使う検証用unitコピーを作り、配布雛形の/usr/bin/nodeという導入先前提と区別する。

## BS2K experimental preview — 2026-10-17

- 2026-10-16最終開発版のBS2K受信・ライブCAS・BS4K統合・取得済み4Kサービス確認を追加。
- 6局設定、EPGStation連携、RAM再投入、失敗を含む日本語資料を追加。
- 配布用bundleをローカルbuildからhash manifest付きで生成。番組streamの終了制御は公開プロキシへ統合し、合成試験を追加。
- 残課題: HDMI復帰、一部暗号化PID、長時間安定性、完全電源断復旧、番組時刻追従。
