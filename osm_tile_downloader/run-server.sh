. .env
export ROCKET_LOG="critical"
export ROCKET_WORKERS="32"

cargo watch -x run --why --delay 1.5 --ignore data.sled --ignore 'data.*' --ignore 'tiles'  --ignore '.tmp'
