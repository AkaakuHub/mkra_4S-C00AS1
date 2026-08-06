# XCode6のABIと選局IPCの実装詳細

フォークする人、他機種へ移植する人以外は読む必要がありません。
使うだけなら [SETUP.md](SETUP.md)、仕組みの概要は [HOW_IT_WORKS.md](HOW_IT_WORKS.md)で足ります。

外部プログラムから本体の4K放送をMPEG-TSとして取り出し、選局まで行うために
必要な全手順です。参照実装はこのリポジトリの`src/`（Rust、依存クレート無し）。

証拠タグ: `[S]`静的解析（バイナリのアドレス付き） / `[M]`実機実測 / `[D]`設計判断

> §5と §9だけは、使うだけの人にも関係します。
> ここに書いてあることを守らないと、本体が「受信できません」表示から復帰しなくなります。

---

## 0. 要約

やることは2つだけである。

1. 製品が動かしているMMTデマルチプレクサのESリングに、mkra独自のREMUXを接続する。
   REMUXはXCode6のハードウェアで、MMT由来のESからMPEG-TSを組み立てる。
   製品は録画時にしかこれを作らないが、同じものをもう一つ作れば録画なしでTSが得られる。
2. 選局は製品自身のチャンネル変更メッセージを送る。
   RFだけ動かしても製品のMMTフィルタが追随しないため、必ず製品に選局させる。

どちらも製品を止めない。実測で60秒連続28.9 Mbps・CCエラー0、
`ffprobe`で`hevc Main10 / 3840x2160 / BT.2020 / HLG + aac_latm`を確認済み。[M]

```
RF ─ SC1501A(ISDB-S3) ─ TSI0 ─ TLV(0x24) ─ MMT_DEMUX(0x25) ┬─ video ES (20 MiB) ─ VIDEO_DECODER
                                                            ├─ audio ES (2 MiB)  ─ AUDIO_DECODER
                                                            └─ MMT-SI section rings ─ abs
                                                                 ▲
                                          自前 REMUX(0x0E) ──────┘ 入力を2本もらう
                                                 └→ TSOUT(LINEAR_RING) → host read → 192B TTS → 188B TS
```

---

## 1. 環境

| 項目 | 値 |
|---|---|
| SoC | ViXS XCode6（`Machine: Xcode6`、ARM Cortex-A9 ×2、Linux 3.10.24） |
| ドライバ | `/dev/vixs/xcodedrv`（`xcdrv.ko` 3.0.0.0.72.00）、ARC700ファーム`xc6800_arc700.bin` 3.48.0.0.72.00 |
| RF | Sony CXD2868ER（`/dev/i2c-0`、7-bit `0x60`） |
| 復調 | SC1501A（`/dev/i2c-0`、page 7-bit `0x10..0x18`）、ISDB-S3 |
| 製品プロセス | `tuner` / `adsb`（MMT・録画）/ `abs`（デコードgraph・UI連携） |
| メモリ | 666 MB。XCode6のhostアパーチャは物理`0x1F000000..0x29000000`（160 MB） |
| 作業領域 | `/tmp/mt`のみ。tmpfsで再起動により消える |

書き込みを禁止している対象: eMMC、MTD、NAND、OTP、`/dev/spidev0.0`（ブートSPI）。
CPLDは非搭載（`[TUNER]:Arib[1]Sky[0]cpld[0]silicon[0]`）なのでselector探索は無意味。[M]

---

## 2. ioctl ABI

`fd = open("/dev/vixs/xcodedrv", O_RDWR)`。呼び出しは`ioctl(fd, cmd, &req)`で、
`req`は入出力兼用のワード配列である。`cmd`の下位16 bitだけがハンドラを選び、
上位16 bitはサブセレクタ（`0x2A` `0x2B` `0x35` `0x36`のみが読む）。[S]

### 2.1使うioctl一覧

| cmd | 意味 | req長 | 備考 |
|---|---|---:|---|
| `0x28` | mini device生成 | 24 B | `[me_type, max_in, max_out, max_ctl, handle(OUT), 0]` |
| `0x29` | mini device破棄 | 4 B | 値渡し（handle） |
| `0x32` | buffer生成 | 36 B | §2.2 |
| `0x34` | buffer破棄 | 4 B | 値渡し |
| `0x37` | reader生成 | 8 B | `[buffer, reader(OUT)]` |
| `0x38` | reader破棄 | 4 B | 先に`0x3F`を呼ぶこと（§5.3） |
| `0x3B` | host read | 40 B | `[reader, wait, timeout, addr, size, addr1, size1, desc, …]` OUT付き |
| `0x3C` | commit read | 16 B | `[reader, addr, size, desc]` |
| `0x3F` | cancel get | 8 B | `[reader, 1]`。`libvixs:xcode_buffer_cancel_get_event @0x86c34` [S] |
| `0x48` | pipeline時刻取得 | 12 B | `[pipeline, lo(OUT), hi(OUT)]` |
| `0x7A` | 物理→ユーザ空間map | 12 B | `[size, phys, vaddr(OUT)]` |
| `0x7B` | unmap | 8 B | `[size, vaddr]`。引数順を間違えるとプロセスのアドレス空間が破損する |
| `0x00010035` | buffer set_ready | 4 B | 値渡し |
| `0x00030035` | buffer force_reset | 8 B | `[buffer, 1]` |
| `0x00010036` | buffer GET_STATUS | 48 B | §2.3 |
| `0x00020036` | buffer GET_READER | `8+4n` B | `access_ok`無しの検証なしのmemcpy。バッファを小さく渡すとoops |
| `0x0001002B` | device get_status | 8 B | `[device, status(OUT)]` |
| `0x0001002A` | device run | 12 B | `[device, wait, timeout_ms]` |
| `0x0003002A` | device stop | 12 B | 同上 |
| `0x0007002A` | connect_input | 16 B | `[device, bind=0, buffer, connect(OUT)]` |
| `0x0008002A` | connect_output | 16 B | 同上 |
| `0x0009002A` | disconnect | 16 B | `[device, connect, wait, timeout_ms]` |
| `0x000C002A` | set_pipeline | 8 B | `[device, pipeline]` |

`0x2A`のサブセレクタは`device_class * 100 + index`（`me_type`ではない）。[S]
REMUXはclass 11。

| コード | 操作 | req長 | 引数 |
|---|---|---:|---|
| `0x044C002A` | conf_remux | 28 B | `[dev, me_format, bf_feature, bitrate, stream_id, pmt_interval, ext]` |
| `0x044D002A` | add_program | 44 B | `[dev, prog, pcr_pid, pmt_pid, pcr_interval, flag, ats_lo, ats_hi, 0,0,0]` |
| `0x044E002A` | remove_program | 8 B | `[dev, prog]` |
| `0x044F002A` | add_video | 28 B | `[dev, connect, prog, pid, stream_id, ext_stream_type, pts_interval]` |
| `0x0450002A` | add_audio | 28 B | 同上 |
| `0x0452002A` | add_psi | 20 B | `[dev, connect, pid, psi_type, interval]` |
| `0x0454002A` | remove_stream | 12 B | `[dev, connect, flag]` |
| `0x0456002A` | config_video | 36 B | `[dev, connect, 0, max_bps, 0,0,0,0, target_bps]` |
| `0x0459002A` | set_copy_permission | 8 B | `[dev, ccc]` |

各コードは`libvixs.so`の該当ラッパー内の`movw r1, #imm`を実測したもの。[S]

### 2.2 `0x32` buffer生成（36 B）

```
[0] me_type        1=CHUNK 3=RING 4=LINEAR_RING
[1] me_media_type  1=SYSTEM 2=DATA 3=VIDEO_ES 6=AUDIO_ES 8=AUDIO_PCM
[2] m_media_subtype
[3] m_max_reader   ★ 生成時固定の消費者クォータ。あとから増やせない
[4] m_alignment
[5] m_size         RING はユニット数、LINEAR_RING はバイト数
[6] m_unit_size
[7] m_bf_flags     bit0/bit1。★ bit1 が立っていると host アパーチャ内に確保される
[8] m_buffer(OUT)
```

`m_max_reader`は引数5番目である（4番目ではない）。ここを間違えると
クォータを`m_media_subtype`に書き込むことになる。[S]

`m_bf_flags` bit1の効果は実測で確認できる。製品のvideo ESはflags=1で
`0x1e02af00`（アパーチャ外＝hostから読めない）、audio ESとTSOUTはflags=3で
`0x28xxxxxx`（アパーチャ内＝`0x7A`でmapできる）。[M]

### 2.3 `0x00010036` GET_STATUS（48 B）

```
[0] m_buffer  [1] me_type   [2] m_mem_size  [3] m_low_threshold
[4] m_high_th [5] m_level   [6] me_media_type [7] ms_media_subtype
[8] m_writer  [9] m_num_readers  [10],[11] 未文書（ES では累積カウンタが入る）
```

クォータ（cap）は返らない。空きがあるかは接続してみるまで分からない。
`-1035 XCE_TOO_MANY`が「枠が満杯」である。[M]

### 2.4主なエラーコード

`-1003` NO_SUPPORT / `-1021`範囲外 / `-1022`無効ハンドル / `-1023`無効パラメータ /
`-1034` reader使用中（cancelしていない）/ `-1035`クォータ超過 / `-1037`状態不正 /
`-1038`タイムアウト / `-1039`キャンセル / `-1040`使用中 / `-1042`存在しない

---

## 3. デバイスとバッファの発見（すべてread-only）

ハンドルを総当たりしてはいけない。ドライバは自分が作った全オブジェクトの表を持つ。[S]

```
gps_XCode（/proc/kallsyms）→ ctx
  ctx+0x4DBD8 count / +0x4DBDC ptr   devices   stride 12  cap 100
  ctx+0x4DBE0 / +0x4DBE4            buffers   stride 12  cap 200
  ctx+0x4DBE8 / +0x4DBEC            pipelines stride  4  cap 32
  ctx+0x4DBF0 / +0x4DBF4            ports     stride 20  cap 300
```

前提: `kptr_restrict=0`、`CONFIG_DEVKMEM=y`（この機種は両方満たす）。
表はlowmemにあるので`/dev/kmem`でそのまま読める。[M]

- device行 = `[handle, me_type, pipeline]`
- port行 = `[port, type, a, b, extra]`。`type==1`は「device aがbuffer bに書く」、
  `type==0`は「device bがbuffer aを読む」、`type==0x0b`はhost接続。

4KのライブESは、MMT_DEMUX（`me_type 0x25`）が書き込むバッファから、次の条件で特定する。

| 対象 | 判定 |
|---|---|
| video ES | `me_media_type==3 && subtype==0 && m_mem_size==0x1400000`（20 MiB） |
| audio ES | `me_media_type==6 && subtype==0 && m_mem_size==0x200000`（2 MiB） |
| pipeline | MMT_DEMUXのdevice行3列目 |

これらは`adsb`のFDCが作る（`videoSetFilter @0x5c178` / `audioSetFilter @0x4a9e4`）。
生成時の`m_max_reader`はvideo 4 / audio 5で、通常はvideo 1 / audio 2しか
使っていない。空きがあるため、外部の読み手を追加できる。[S][M]

---

## 4. mkra独自のREMUXを組む手順

製品`adsb:remux_xcif_open_remux @0x421f8`ほかの録画用レシピから、録画部分を除いたもの。
順序も製品どおりにする。

```c
/* 1. REMUX 本体 */
ioctl(0x28, [0x0E, 13, 2, 1, &remux, 0]);              /* adsb:0x42260 */

/* 2. 全体設定。m_stream_id が生成される PAT の TSID になる */
ioctl(0x044C002A, [remux, 3, 0x10, 0, TSID, 100, 0]);  /* adsb:0x42294 */

/* 3. 出力リング。flags=3 で host アパーチャに確保させる */
ioctl(0x32, [4, 1, 0, 1, 0x1000, 0x468000, 1, 3, &tsout]);   /* adsb:0x422e2 */

/* 4. 出力接続（TSOUT だけ。TSINFO は繋がない → §5.1） */
ioctl(0x0008002A, [remux, 0, tsout, &conn_out]);

/* 5. 製品のライブ ES を入力にもらう。ここが -1035 ならクォータ満杯 */
ioctl(0x0007002A, [remux, 0, video_es, &conn_v]);
ioctl(0x0007002A, [remux, 0, audio_es, &conn_a]);

/* 6. 基準時刻。製品は選局時の参照時刻を使うが pipeline 時刻で足りる */
ioctl(0x48, [pipeline, &lo, &hi]);

/* 7. プログラムとストリームの宣言 */
ioctl(0x044D002A, [remux, SID, 0x1001, 0x0100, 100, 1, lo, hi, 0,0,0]);
ioctl(0x0456002A, [remux, conn_v, 0, 0x02A4C200, 0,0,0,0, 0x029F6300]);
ioctl(0x044F002A, [remux, conn_v, SID, 0x1011, 0xE0, 0x24, 0]);   /* HEVC */
ioctl(0x0450002A, [remux, conn_a, SID, 0x1100, 0xC0, 0x11, 0]);   /* AAC-LATM */

/* 8. 起動 */
ioctl(0x0459002A, [remux, 0]);          /* copy permission */
ioctl(0x000C002A, [remux, pipeline]);   /* 製品のライブ pipeline に載せる */
ioctl(0x00010035, tsout);               /* set_ready */
ioctl(0x0001002A, [remux, 1, 2000]);    /* run */

/* 9. 読み出し */
reader = ioctl(0x37, [tsout, &reader]);
loop {
    ioctl(0x3B, [reader, 1, 500, &addr, &size, &addr1, &size1, &desc]);
    /* addr の扱いは §5.2 */
    ioctl(0x3C, [reader, addr, size, desc]);
}

/* 10. 後始末（製品と同じ順序） */
ioctl(0x0003002A, [remux, 1, 2000]);                 /* stop */
ioctl(0x0454002A, [remux, conn_v, 1]);               /* remove_stream */
ioctl(0x0454002A, [remux, conn_a, 1]);
ioctl(0x044E002A, [remux, SID]);                     /* remove_program */
ioctl(0x0009002A, [remux, conn_v, 1, 2000]);         /* disconnect ×3 */
ioctl(0x0009002A, [remux, conn_a, 1, 2000]);
ioctl(0x0009002A, [remux, conn_out, 1, 2000]);
ioctl(0x3F, [reader, 1]);  ioctl(0x38, reader);      /* cancel してから破棄 */
ioctl(0x34, tsout);
ioctl(0x29, remux);
```

この3つを省くと出力が1 byteも出ない（実測）: `0x48`の時刻を`add_program`の
ATS基準に渡すこと、`config_video`、`set_copy_permission`。[M]

`remove_stream`が`-1042`を返すことがあるが、stop後に呼ぶ製品と同じ順序であり、
graphは毎回きれいに戻る。[M]

---

## 5. 注意事項

### 5.1 TSINFOを接続してはいけない

REMUXは`m_max_output=2`で作るが、出力はTSOUT 1本だけ接続すれば動く。
製品はATS索引用にTSINFOも接続するが、外部から接続するとドレインの有無に関わらず
数秒でREMUXが入力を取らなくなる。device statusは`0x2`(running)のままで、
`run`を再実行しても復帰しない。[M]

| 条件 | 結果 |
|---|---|
| TSINFO接続・未ドレイン | 約2秒（7–9 MB）で停止 |
| TSINFO接続・ドレインあり | 2.5–10秒（8–36 MB）で停止、停止時刻は非決定的 |
| TSINFO未接続 | 60秒217 MBまで停止なし |

### 5.2 `0x3B`の返すアドレスは物理とは限らない

hostメモリに確保されたバッファでは、既に自プロセスへmap済みのポインタが返る。
製品`abs:remuxif_task @0x2b89fe`も返り値をそのまま参照している。[S]
これを物理として`0x7A`でmapし直すと、無関係な物理ページを読んで
放送データに似た無関係な値が取れる（ARMのコードやゼロ領域が混ざった0x80埋めが出る）。[M]

対処として、 `/proc/self/maps`に含まれるアドレスなら直接読み、含まれなければ`0x7A`でmapする。

### 5.3 `0x38`の前に`0x3F`を呼ぶ

`get_read`で待っているreaderは破棄できず`-1034`を返す。その後buffer破棄も
`-1040 XCE_IN_USE`になり、毎回4.6 MBずつリークする（電源を切るまで戻らない）。
製品もhost connectionを閉じる前に`0x3F`を呼んでいる（`abs:0x2b84b8`）。[S][M]

### 5.4読み出しが止まったtapによる復旧の阻害

tapが消費を止めると、ESリングが滞留 → デコーダ枯渇（`HDMI UFLOW`/`SPDF UFLOW`）
→ `adsb`がESバッファを張り替えようとしてmkraの接続が残っているため
`XC_SBUF_DESTROY`が`XCE_IN_USE(-1040)`で失敗 → `ADSB Tuning Error` →
画面が「受信できません」になる。シリアルログで因果を確認済み。[M]

対処として、ライブvideo ESの`m_level`を監視し、滞留したら即disconnectする。
定常は9〜14、事故時は0x65まで上がっていた。閾値48で十分な余裕がある。[D]
通常のgapは150〜390 msで自然回復するので、時間だけで判断してはいけない。

接続を作る前にも映像ESの滞留量を確認し、`--es-level-abort`の閾値以上なら作成を拒否する。読み出しの`Err`はセッションを終了して接続を解除する。通常の一時的な待ち時間は、読み出し側が`Ok(0)`として返す。

pipeline時刻の取得に失敗した場合は、REMUXの作成を中止する。時刻0で処理を続けたり、作成に失敗したREMUXを再試行したりしない。

### 5.5製品バッファへの`create_reader`禁止

`0x37`は成功しても`0x38`で外せないことがあり、枠が1つ永久に減る。
device接続（`0x0007002A`）は`disconnect`で正常に外れるので、
製品のバッファに触るときは必ずdevice接続を使う。[M]

### 5.6アパーチャ外は読めない

TLVリング（`0x1df.....`、4 MiB ×4）とvideo ES（20 MiB）は
hostアパーチャ`0x1F000000..0x29000000`の外にある。生のTLVを読み出して
PC側でMMT→TS変換する経路は取れない。[M] だからREMUXを使う。

---

## 6. 出力フォーマット

- 192 byte TTS（`[4 byte 到着時刻][188 byte TS]`）。位相4、同期率1.0000。[M]
  `me_format=3`の結果である。188にしたければ4 byteを捨てる。
- REMUXがPAT/PMTも生成する。TSIDは`conf_remux`の`m_stream_id`、
  program numberは`add_program`の値。`add_psi`でセクションリングを接続する必要はない。[M]
- 生成されるPMT: `PCR 0x1001 / stream_type 0x24 (HEVC) PID 0x1011 /
  stream_type 0x11 (AAC-LATM) PID 0x1100`。
- 実測レート27〜29 Mbps、60 fps、CCエラー0、`ffprobe`で
  `hevc Main10 3840x2160 yuv420p10le BT.2020 arib-std-b67(HLG)` + `aac_latm`。[M]

実際のサービスIDを設定すること。 `m_stream_id`に実TLVストリームID、
`add_program`に実サービスIDを渡すと、Mirakurun系クライアントが
サービス単位でフィルタできるTSになる。[D]

---

## 7. 選局（製品IPC）

### 7.1 RFの直接操作では取得できない理由

製品のMMTフィルタは選局中サービスのアセット向けに構成されている。RFだけ別
TLVストリームへ動かすとESリングが空になる。製品自身に選局させれば全段を作り直す。[S][M]

```
eAdsbCommProcChangeChannel → msgAdsbCommRequestTumChangeChannel
  → eAdsbTumFSDTVTuningStart → fdc_chmCancelSelectChannel → fdc_chmDisConnectTsUnits
  → TUNER_ADBS1 OFF → RfStandby → OfdmStandby → FemDisconnectDevice ×3
  → fdc_chmConnectTsUnits(source:100 destination:0) → fdc_chmSelectChannel
  → psfc_fed_AdbsSetTuningStart(tuner=1, freq=<IF kHz>, streamid=<TSID>,
                                inputPortNo=100, msgqid=896)
  → FemChangeDevice → SetTsid/SetFreq/SeqStart → @@@lock
  → [REMUX] NetworkID/ServiceID → MPT/CCC → ES 再構成 → デコード再開
```

### 7.2キューの解決

`/tmp/bdh_out_msgh.lst`が`qid-msqid`の対応表。msqidは起動ごとに変わるので
毎回読むこと。ADSB通信キューは`qid = 0x36F`(879)。[M]

### 7.3メッセージ形式（SysV `msgsnd`）

```c
struct wire {                 /* msgsnd に渡す長さは 92（mtype を除く） */
    long mtype;      /* +0x00  常に 1 */
    u32  _unused[2]; /* +0x04  製品は未初期化のまま送る */
    u32  self_ptr;   /* +0x0c  受信側は使わない。0 でよい */
    u32  _inner;     /* +0x10 */
    u32  msg_id;     /* +0x14 */
    u32  len;        /* +0x18  payload の有効長 */
    u32  src;        /* +0x1c */
    u8   payload[64];/* +0x20 */
};
```

この配置は`/proc/<tid>/syscall`で製品タスクの`msgrcv`引数を読み、
`/proc/<pid>/mem`から受信バッファを採取して実物と一致することを確認済み。[M]

### 7.4チャンネル変更要求

```
msg_id = 0x6004    宛先 qid = 0x36F    len = 0x28（10 word）

payload = [ 0, OID, 9, 0, 0x0b, SID, RF_kHz, TSID, 0, BID ]
             │   │  │  │    │    │     │      │    │   └ リモコンキー番号
             │   │  │  │    │    │     │      │    └ NotDecode（0）
             │   │  │  │    │    │     │      └ TLV ストリーム ID
             │   │  │  │    │    │     └ RF 周波数 kHz（IF ではない）
             │   │  │  │    │    └ サービス ID
             │   │  │  │    └ network id（advanced BS は 0x0b）
             │   │  │  └ Tune（0）
             │   │  └ Dev（9）
             │   └ 操作連番。受信側がエコーするだけ
             └ 常に 0
```

根拠: 送信側`abs:vEtmMain+0x308 @0x459a64`が10 wordを組み
`eSysSendMessage(0x36F, 0x6004, body, 0x28, -1)`を`0x459af2`で発行する [S]。
受信側はこのbodyをそのままログへ出すので、リモコンで10波すべて切り替えた
シリアルログと突き合わせて確認した [M]。

### 7.5全10波の対応表 [M]

| SID | 局 | RF kHz | TSID | BID |
|---:|---|---:|---|---:|
| 101 | NHK BSP4K | 12,034,360 | 0xB110 | 1 |
| 102 | NHK BS8K | 11,976,820 | 0xB0E0 | 1 |
| 141 | BS日テレ4K | 11,842,560 | 0xB072 | 2 |
| 151 | BS朝日4K | 11,842,560 | 0xB070 | 3 |
| 161 | BS-TBS4K | 12,034,360 | 0xB111 | 4 |
| 171 | BSテレ東4K | 11,842,560 | 0xB071 | 5 |
| 181 | BSフジ4K | 12,034,360 | 0xB112 | 6 |
| 211 | ショップチャンネル | 11,919,280 | 0xB0B0 | 9 |
| 221 | 4K QVC | 11,919,280 | 0xB0B2 | 0xa |
| 231 | （名称なし） | 11,919,280 | 0xB2B1 | 0xb |

IFは`RF − 10,678,000` kHz（LNB LO）。製品のFEDメッセージはIFを使うが、
このチャンネル変更要求はRFを渡す。

### 7.6成功判定

応答先として使用するキューを設定できないので、送信結果では判定できない。
製品自身のSIが新サービスになったことを見る（約4秒）。§8参照。

### 7.7順序の制約

選局の前にライブESのtapを必ず外す。残っていると製品側のESバッファ破棄が
`XCE_IN_USE`で失敗し、§5.4の停止に至る。選局 → SI確認 → tap再構築の順にする。[D]

---

## 7.8スタンバイと復帰

スタンバイではvideo/audio ESリングが解放されるので、その状態では配信できない。
残るのはTLV(0x24)とMMT_DEMUX(0x25)だけで、製品はスタンバイ中も選局してEPGを集める。[M]

| | 復号中 | スタンバイ |
|---|---:|---:|
| graph devices | 22 | 11 |
| ARM CPU busy | 54.4% | 51.8% |

4K復号のARM負荷は2.6ポイントしかない（復号はXCode6のハードウェア）。[M]

起床は同じ0x6004メッセージで行える。 `Tune = 3`（製品自身も電源オン復帰でこの値を使う）
を送ると、3秒で復号チェーンとESリングが再生成され、そのまま配信できる。[M]

```
before : devices 11 / buffers 9
mkra tune4k --sid 141 --tune-mode 3
t+3s   : devices 22 / buffers 24 → 直後の採取で 23.3 MB / 6 秒 / 同期率 1.0000
```

`--mac-wol-idle <sec>`は、指定秒数の間受信と確立済みのTCP接続がない場合、純正LAN ioctlでMACだけをMagic Packet待機へ移します。Linuxと純正の電源管理処理は動作し続けます。RF休止用の`--idle-sleep`と`fed-off`は、純正FEDの返信キューを詰まらせたため削除しました。[電源管理の記録](../tools/contrib/power/README.md)を参照してください。

旧版の無信号トランスポンダーへの選局は廃止しました。現行の処理、実測したレジスター値、本体全体の待機との違いは[電源管理の記録](../tools/contrib/power/README.md)を参照してください。

製品のスタンバイと同じ状態へ移行する方法は未確立。 `NotDecode = 1`は「製品の電源状態がstandbyのとき」しか効かず、
通常の稼働状態では受理されるが無視される。[M] 製品の電源状態は`abs`内のUIM/INI
状態機械が持っており未解読である。

純正のスタンバイに入った本体を配信要求で起こせることは確認できていません。現在確認したMagic Packetによる復帰は、Linuxを動かしたままのMAC待機に限ります。

## 8. 選局中サービスの特定

`adsb`が`/tmp/adsb`に書くMMT-SIをそのまま読む。ioctlも不要。[M]

```
AdsbSiMhSdtActual.dat   MH-SDT actual（選局中の TLV ストリーム）
  9f f0 59 | b0 72 | c3 00 00 | 00 0b | ff | 00 8d | ...
  ^tid      ^TLV stream id     ^network id      ^service id
   (+0)      (+3,+4)            (+8,+9)          (+11,+12)

AdsbSiMpt.dat           選局中パッケージの MPT のみ
  20 | 54 | 03 76 | fc | 02 | 00 8d
  ^tid ^ver ^len    ^mode ^idlen ^MMT_package_id = service id
```

両者が一致したときだけ採用する。不一致は選局遷移中とみなす。[D]
セクションはCRC-32/MPEG-2が0になることで検証できる（MH-SDTはlong form）。

番組表も同じディレクトリから作れる（`AdsbSiMhEitPF.dat` / `MhEitSch*.dat`）。
実測297番組、日本語タイトル・`2160p`・`BT.2100(HLG)`付きで取れる。[M]

---

## 9. 実機試験の安全規則

この機種で障害が発生した、または発生しかけた実例に基づく規則である。

1. ハンドルを総当たりしない。ドライバのgraphから取る。総当たりはkernel oopsの実績あり。
2. `0x00020036` GET_READERは`access_ok`無しの検証なしのmemcpy。ベンダの
   `xcode_buffer_get_readers`ラッパーはスタック16 byteしか確保しないので、
   `m_num >= 2`で呼ぶとスタックを壊す。mkra独自のでリクエストを組むこと。
3. 未検証の物理アドレスを`0x7A`でmapしない（アパーチャ外はバスが応答しない）。
4. 製品のdevice / bufferを破棄しない。触るのは自分が作ったものだけ。
5. 実機に置くファイルは`/tmp/mt`だけ。
6. 一度の実機試験で判定する仮説は1つ。値の総当たりをしない。
7. 動作中の`tuner`とI2Cを競合させない（共有セマフォshm key `0x3400103B`）。
8. 復旧は再起動より先にリモコン選局／待機→電源オンを試す。この状態でもRF受信は動作していることが多い。

---

## 10. 検証方法と実測値

| 確認したいこと | 方法 | 実測 |
|---|---|---|
| TSが壊れていないか | 188 byte同期率、PIDごとのCCエラー | 60秒217 MBでCCエラー0 |
| 取りこぼしが無いか | PCRの時間幅 ÷ 実時間 | 20.405 s / 20 s = 1.00 |
| 実際に復号できるか | `ffprobe` / `ffmpeg -frames:v 1` | 3840×2160の実画像を取得 |
| 製品を壊していないか | graphのdevice/buffer/port数、ESのreaders | 実行前後で完全一致 |
| クライアントが追いつけているか | サーバ側のクライアント別ドロップ率 | 有線Macで0.000% |
| 選局できたか | `/tmp/adsb/AdsbSiMhSdtActual.dat` | 要求から約4秒で切替 |

参考: `mkra`の対応コマンド（実機の`/tmp/mt/mkra`）

```sh
mkra graph --sniff                 # ドライバの live graph を読む（read-only）
mkra remux4k --discover-only       # ライブ ES と pipeline を報告して終了
mkra remux4k --out x.ts --seconds 20
mkra tune4k --sid 141 [--dry-run]  # 製品に選局させる
mkra serve --remux4k --tune --channel-type BS4K --port 40772 --epg-dir /tmp/adsb
```

---

## 11. 他機種へ移植する際の注意事項

- XCode6のioctl番号とリクエスト長はドライバ版に依存する。 `xcdrv.ko`は
  ET_RELで関数名が残っているので、各ハンドラが`+0x8b8`に書く即値がリクエスト長である。
  具体的には各ハンドラがrequest長を1 byteの即値として`+0x8b8`へ`strb`するので、
  逆アセンブルして直前の`mov`の即値を読めばよい。
- 製品IPCのキューID・メッセージIDは機種ごとに違う。手順は同じで、
  (1)シリアルコンソールで製品のログを取り、(2)送信側関数を逆アセンブルして
  `eSysSendMessage(qid, msgId, body, len, …)`を読み、(3) `/proc/<tid>/syscall` +
  `/proc/<pid>/mem`で実物のメッセージと突き合わせる。
- 動作したように見える結果でも、有効な放送データか確認すること。この機種では
  「TSIがlockしてCRCが進む」も「get_readがdataを返す」も、
  有効なMPEG-TSの証拠にはならなかった。最終的な証拠は常に
  「映像をデコードできるか」「CCエラーが0か」である。
- **XCode6 の ioctl 番号とリクエスト長はドライバ版に依存する。** `xcdrv.ko` は
  ET_REL で関数名が残っているので、各ハンドラが `+0x8b8` に書く即値がリクエスト長である。
  具体的には各ハンドラが request 長を 1 byte の即値として `+0x8b8` へ `strb` するので、
  逆アセンブルして直前の `mov` の即値を読めばよい。
- **製品 IPC のキュー ID・メッセージ ID は機種ごとに違う。** 手順は同じで、
  (1) シリアルコンソールで製品のログを取り、(2) 送信側関数を逆アセンブルして
  `eSysSendMessage(qid, msgId, body, len, …)` を読み、(3) `/proc/<tid>/syscall` +
  `/proc/<pid>/mem` で実物のメッセージと突き合わせる。
- **「動いたように見えて実は違う」を疑うこと。** この機種では
  「TSI が lock して CRC が進む」も「get_read が data を返す」も、
  有効な MPEG-TS の証拠にはならなかった。最終的な証拠は常に
  「復号できる画が出るか」「CC エラーが 0 か」である。

2026-10-15訂正: REMUX remove_streamの第3ワードはflagではなく
`m_program_number`。libvixsの0x7f8c4が参照するログ文字列0x120d34で確認。
動的なservice_idを番組番号に使う場合、削除も同じ番号を渡す。
純正のliteral 1をそのまま移すと番組101等の削除時に1042エラーとなる。

### Owned LOCAL_DEMUX memory-input diagnostic (2026-10-16)

This is a positive control with saved transport data, not a BS2K RF capture.
The OEM LOCAL_DEMUX create request is `[6, 1, 48, 0, OUT, 0]`.
The 44-byte host get_write request is
`[writer, size, flags, descriptor_size, wait, timeout, addr, size0, addr1, size1, desc]`.
Word 3 requests 128 descriptor bytes; it is not reserved. The descriptor is 32
u32 words: word 0=128, word 1=returned address (the native sequence-marker
assignment), word 3=0x100, words 29..31=`[12, 1, subtype]`, all others zero.
SYSTEM subtype 2 is TS, subtype 3 is TTS. No timestamp flags are asserted.
The returned descriptor must be a mapped, aligned 128-byte user range.
Commit is `[writer, addr, size, desc, 0, 0]` (24 bytes).

With descriptors, recorded 188-byte input was reproduced byte for byte;
192-byte timestamped input produced exactly the same input with each four-byte
timestamp removed. A separate PID0 filter produced seven CRC-valid PAT sections.
Without descriptors all these LOCAL_DEMUX outputs were empty. This does not
establish the physical TSI input path used by type12 DEMUX.
See BS2K-EXPERIMENT-LOG.ja.md for controls, binaries, and evidence hashes.
