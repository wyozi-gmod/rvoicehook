# rvoicehook

```lua
hook.Add("VoiceData", "test", function(cl, da)
  print("Received ", #da, "bytes of PCM voice data from client at slot", cl)
end)
```

## Compile

```
PKG_CONFIG_ALLOW_CROSS=1 cargo build --target i686-unknown-linux-gnu && cp target/i686-unknown-linux-gnu/debug/librvoicehook.so ./srcds/garrysmod/lua/bin/gmsv_rvoicehook_linux.dll
```
