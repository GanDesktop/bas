# mTLS 配置

BAS 支持双向证书认证。服务端验证客户端 CA，客户端验证服务端 CA 及目标的 DNS/IP SAN。
所有获准客户端共享该实例的执行环境；证书不提供实例内用户隔离。

## 准备证书

已有 CA 时使用其签发的证书。以下示例在私有目录中创建 CA 与有效期 30 天的服务端、客户端证书；
将域名和 IP 替换为实际地址，每个客户端使用独立密钥。

```sh
umask 077
mkdir bas-certificates
cd bas-certificates

# Keep this encrypted CA key offline; never give it to the daemon or clients.
openssl req -x509 -newkey rsa:3072 -sha256 -days 365 \
  -keyout ca.key -out ca.pem -subj "/CN=Bas private CA" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"

openssl req -new -newkey rsa:3072 -nodes \
  -keyout server.key -out server.csr -subj "/CN=Bas server"
printf '%s\n' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature,keyEncipherment' \
  'extendedKeyUsage=serverAuth' \
  'subjectAltName=DNS:bas.example.internal,IP:192.0.2.10' > server.ext
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -days 30 -sha256 -extfile server.ext -out server.pem

# Issue a separate client certificate/key for each client.
openssl req -new -newkey rsa:3072 -nodes \
  -keyout client.key -out client.csr -subj "/CN=Bas client workstation"
printf '%s\n' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature' \
  'extendedKeyUsage=clientAuth' > client.ext
openssl x509 -req -in client.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -days 30 -sha256 -extfile client.ext -out client.pem
```

服务端需要 `server.pem`、`server.key`、客户端 CA；客户端需要 `client.pem`、`client.key`、服务端 CA。
CA 私钥不分发给任何端点。证书为 PEM，链按叶证书、再中间证书排列；密钥文件只允许所属用户读取。
参数使用文件路径，建议绝对路径。端点私钥需可无人值守读取。

## 启动

```sh
bas --listen local --listen mtls://0.0.0.0:7338 \
  --tls-certificate /private/bas/server.pem \
  --tls-key /private/bas/server.key \
  --tls-client-ca /private/bas/ca.pem

bast --server mtls://bas.example.internal:7338 \
  --tls-ca /private/client/ca.pem \
  --tls-certificate /private/client/client.pem \
  --tls-key /private/client/client.key agents
```

使用不同网络地址访问同一服务时，可用 `--tls-server-name` 指定证书验证名称，网络目标保持不变。
TLS 参数只作用于 `mtls://` 入口；缺少证书配置时启动失败，不回退到明文。
多入口共享状态，明文 TCP 入口仍是无认证访问。

## 换证与撤销

服务端在启动时加载证书和 CA，修改后重启；客户端在创建 Channel 时加载，换证后需创建新连接。
当前没有设备撤销列表或 CRL/OCSP。受信任 CA 签发的有效客户端证书均可访问服务。
撤销整个信任域需要替换客户端 CA、重新签发授权设备证书并重启服务；已有长连接不会逐请求重验。

设备配对与撤销规划见[远程接入](design/remote-access.md)。
