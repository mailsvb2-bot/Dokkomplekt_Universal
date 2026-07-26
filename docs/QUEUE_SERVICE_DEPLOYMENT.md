# Центральная очередь: desktop/offline-first deployment

Доккомплект на рабочем компьютере **не подключается к PostgreSQL**. Обычный режим полностью локальный: SQLite состояния, файловая SHA-256 очередь, OCR, локальная модель, генерация и печать продолжают работать без интернета.

Центральная очередь включается только для нескольких компьютеров. Схема:

```text
Desktop workers -- HTTPS + client certificate --> queue_mtls_service.py --> SQLite or PostgreSQL/TLS
```

Desktop хранит только адрес сервиса, публичный CA и собственный клиентский сертификат. Пароль базы находится только на сервере очереди.

## Небольшой офис: SQLite backend

```bash
python scripts/queue_mtls_service.py \
  --host 0.0.0.0 --port 9443 \
  --database /var/lib/dokkomplekt-queue/queue.sqlite3 \
  --server-cert /etc/dokkomplekt-queue/server.crt \
  --server-key /etc/dokkomplekt-queue/server.key \
  --client-ca /etc/dokkomplekt-queue/client-ca.crt
```

Этот вариант рассчитан на один экземпляр queue service. SQLite использует WAL, `synchronous=FULL` и транзакционный `BEGIN IMMEDIATE`.

## Production/HA: PostgreSQL backend

Установить только на сервере очереди:

```bash
python -m pip install -r requirements-queue-server.txt
```

DSN хранить в файле с правами `0600`, например `/etc/dokkomplekt-queue/postgres.dsn`:

```text
host=db.internal dbname=dokkomplekt_queue user=queue_service password=... connect_timeout=5 sslmode=verify-full sslrootcert=/etc/dokkomplekt-queue/db-ca.crt sslcert=/etc/dokkomplekt-queue/db-client.crt sslkey=/etc/dokkomplekt-queue/db-client.key
```

Запуск:

```bash
python scripts/queue_mtls_service.py \
  --host 0.0.0.0 --port 9443 \
  --postgres-dsn-file /etc/dokkomplekt-queue/postgres.dsn \
  --server-cert /etc/dokkomplekt-queue/server.crt \
  --server-key /etc/dokkomplekt-queue/server.key \
  --client-ca /etc/dokkomplekt-queue/client-ca.crt
```

Сервис fail-closed отклоняет PostgreSQL DSN без `sslmode=verify-full|verify-ca` и `sslrootcert`. Пара `sslcert`/`sslkey` должна быть задана целиком; приватный ключ на Unix не может быть доступен группе или остальным пользователям.

## Настройка desktop worker

```text
DOKKOMPLEKT_QUEUE_MTLS_URL=https://queue.example.internal:9443
DOKKOMPLEKT_QUEUE_MTLS_CA_PEM=C:\ProgramData\Dokkomplekt\queue-ca.pem
DOKKOMPLEKT_QUEUE_MTLS_IDENTITY_PEM=C:\ProgramData\Dokkomplekt\queue-client-combined.pem
```

Если эти переменные отсутствуют, queue service не вызывается вообще. Потеря интернета или сервера не влияет на обычную локальную работу. Если пользователь явно включил центральный режим, его недоступность блокирует только распределённую обработку fail-closed, чтобы два компьютера не опубликовали один комплект одновременно.
