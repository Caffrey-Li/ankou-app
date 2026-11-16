#!/bin/bash
# 在登录钥匙串里生成一个只在这台 Mac 上用的自签名代码签名证书「Wanxiang Local Signing」。
# 用它签名后，每次重新编译，应用的签名身份保持不变，钥匙串里点过的「始终允许」会一直有效。
# 已经存在就跳过。私钥只放在登录钥匙串，不导出到别处；临时文件用完即删。
set -euo pipefail
NAME="Wanxiang Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
if security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "证书已存在：$NAME"
else
  WORK=$(mktemp -d)
  trap 'rm -rf "$WORK"' EXIT
  cat > "$WORK/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
  /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$WORK/key.pem" -out "$WORK/cert.pem" -config "$WORK/cert.cnf" 2>/dev/null
  PASS=$(/usr/bin/openssl rand -hex 12)
  /usr/bin/openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
    -name "$NAME" -out "$WORK/id.p12" -passout "pass:$PASS" 2>/dev/null
  security import "$WORK/id.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null
  echo "已生成证书：$NAME"
  echo "下面系统会请你输入一次登录密码，用来把这个证书标记为可用于代码签名。"
  security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$WORK/cert.pem"
fi
echo "可用的签名身份："
security find-identity -p codesigning "$KEYCHAIN" | grep -F "$NAME" || echo "（没有找到，请把这段输出发给 Claude）"
