# Windowsから機器のIPアドレスを調べる

`find_device_windows.py`は、指定したMACアドレスと一致する機器を同じIPv4サブネットから探す。Python標準ライブラリとWindowsの[SendARP API](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-sendarp)を使用する。ルーターを越える探索はできない。

`ipconfig`でPCのIPv4アドレスとサブネットを確認し、対象機器のMACアドレスとともに実行時の引数へ指定する。

```powershell
python tools/contrib/network/find_device_windows.py --mac "機器のMACアドレス" --source "PCのIPv4アドレス" --network "ネットワークアドレス/プレフィックス長"
```

指定したMACアドレスに一致する機器が見つかると、IPv4アドレスを標準出力へ表示して終了コード0を返す。見つからない場合は1を返す。

探索は32並列で行う。ほかの機器のMACアドレスは出力せず、アドレスや探索結果をファイルにも保存しない。

telnet有効化済みのAS1へ接続する場合は、表示されたIPアドレスとTCPポート2323を指定する。
