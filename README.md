# PTZ Camera Controller for Logitech Rally (Rust)

Высокопроизводительное приложение на **Rust** для полноценного управления PTZ-камерами **Logitech Rally** (а также Logitech PTZ Pro, PTZ Pro 2, MeetUp и другими UVC PTZ камерами) в среде Linux / Ubuntu.

Проект объединяет возможности референса [xMRi/PTZControl](https://github.com/xMRi/PTZControl) (Windows UVC XU Extension Units) и управления через Video4Linux2 (`v4l2`), описанного в руководстве *"Accessing Logitech Camera controls in Ubuntu"*.

---

## 🚀 Основные возможности

1. **Автоматическое обнаружение оборудования**:
   - Автопоиск подключенных камер через V4L2 (`/sys/class/video4linux` и `ioctl VIDIOC_QUERYCAP`).
   - Автоматический выбор `Logi Rally Camera` без необходимости вручную указывать путь к устройству.
2. **Точное PTZ-управление (Pan, Tilt, Zoom)**:
   - **Pan (Панорама)**: от `-90.0°` до `+90.0°` (с точностью до 1 угловой секунды / 3600 единиц на градус).
   - **Tilt (Наклон)**: от `-90.0°` до `+50.0°`.
   - **Zoom (Оптический зум)**: от `1.0x` до `15.0x`.
   - **Home Position**: аппаратный и программный возврат в нулевую точку (`0.0°`, `0.0°`, `1.0x`).
3. **Устранение мерцания (Anti-Flicker / Power Line Frequency)**:
   - Переключение частоты питания `50 Hz` (для Европы / СНГ) / `60 Hz` (США) / `Disabled` для устранения полос на видео при искусственном освещении (решение проблемы из статьи Himanshu Chaudhary).
4. **Аппаратные и программные пресеты (Presets)**:
   - **Hardware Presets (1–8)**: сохранение и вызов позиций напрямую из встроенной памяти EEPROM камеры через команды UVC Extension Unit (Unit 11, Selector 2).
   - **Software Named Presets**: сохранение неограниченного числа именованных позиций (например, `podium`, `whiteboard`, `audience`) в `~/.config/ptz-rally/presets.json` вместе с настройками яркости и зума.
5. **Интерактивный терминальный интерфейс (TUI)**:
   - 2D-прицел (Reticle) текущего положения камеры в реальном времени.
   - Визуальные шкалы Pan, Tilt, Zoom.
   - Управление стрелками клавиатуры, клавишами `+`/`-`, горячими клавишами `1-8` и `Home`.
6. **Встроенный HTTP REST API**:
   - Позволяет интегрировать камеру с **Bitfocus Companion**, **Elgato Stream Deck**, **OBS Studio**, веб-интерфейсами или системами автоматизации (Home Assistant).
7. **Совместимость с CLI флагами xMRi/PTZControl**:
   - Поддерживает флаги `-move_up`, `-move_down`, `-move_left`, `-move_right`, `-zoom_in`, `-zoom_out`, `-move_home`, `-store:<N>`, `-restore:<N>`.

---

## 📦 Установка и сборка

Приложение собрано и установлено в `~/.local/bin/ptz-rally`.

Для пересборки из исходников:
```bash
cargo build --release
cp target/release/ptz-rally ~/.local/bin/
```

---

## 🕹️ Способы использования

### 1. Интерактивный TUI интерфейс

Запуск визуального терминала управления:
```bash
ptz-rally tui
# или просто:
ptz-rally
```

**Горячие клавиши в TUI:**
- `←` / `→` — поворот влево / вправо (Pan)
- `↑` / `↓` — наклон вверх / вниз (Tilt)
- `+` / `-` или `PageUp` / `PageDown` — приближение / отдаление (Zoom)
- `Home` или `h` или `0` — возврат в домашнюю позицию
- `1` .. `8` — быстрый вызов аппаратного пресета (Recall Preset 1–8)
- `Shift + 1..8` — сохранить текущее положение в аппаратный пресет (Save Preset 1–8)
- `F` — переключить подавление мерцания (Off -> 50Hz -> 60Hz)
- `[` / `]` — изменить шаг перемещения (1°, 5°, 10°, 20°)
- `B` / `b` — регулировка яркости (Brightness)
- `C` / `c` — регулировка контраста (Contrast)
- `Q` / `Esc` — выход

---

### 2. Командная строка (CLI)

#### Просмотр статуса и списка устройств
```bash
# Список всех камер
ptz-rally list

# Текущее положение и настройки
ptz-rally status

# Вывод в формате JSON (удобно для скриптов)
ptz-rally status --json
```

#### Управление панорамой (Pan)
```bash
# Повернуть влево на 5 градусов
ptz-rally pan --left --step 5.0

# Повернуть вправо на 10 градусов
ptz-rally pan --right --step 10.0

# Установить абсолютный угол (+15.0°)
ptz-rally pan --to 15.0
```

#### Управление наклоном (Tilt)
```bash
# Наклонить вверх на 3 градуса
ptz-rally tilt --up --step 3.0

# Наклонить вниз
ptz-rally tilt --down --step 3.0

# Установить абсолютный угол (-10.0°)
ptz-rally tilt --to -10.0
```

#### Управление зумом (Zoom)
```bash
# Приблизить на 0.5x
ptz-rally zoom --zoom-in --step 0.5

# Отдалить
ptz-rally zoom --zoom-out --step 0.5

# Установить зум 3.5x
ptz-rally zoom --to 3.5
```

#### Домашняя позиция (Home)
```bash
ptz-rally home
```

#### Одновременное перемещение Pan + Tilt
```bash
ptz-rally move --pan 20.0 --tilt -5.0
ptz-rally move --pan-delta -5.0 --tilt-delta 2.0
```

#### Пресеты (Presets)
```bash
# Аппаратные пресеты (Logitech EEPROM 1–8):
ptz-rally preset --save 1       # Сохранить пресет 1
ptz-rally preset --recall 1     # Вызвать пресет 1

# Именованные программные пресеты:
ptz-rally preset --save-soft podium
ptz-rally preset --recall-soft podium
ptz-rally preset --list-soft
ptz-rally preset --delete-soft podium
```

#### Настройки изображения и устранение мерцания
```bash
# Устранение мерцания (50 Гц для Европы/СНГ)
ptz-rally set --flicker 50

# Настройка яркости (0-255)
ptz-rally set --brightness 130

# Настройка контраста
ptz-rally set --contrast 128
```

---

### 3. HTTP REST API сервер (для Stream Deck / Bitfocus Companion)

Запуск фонового сервера:
```bash
ptz-rally serve --port 8080
```

**Доступные REST эндпоинты:**
| Метод | URL | Описание |
|---|---|---|
| `GET` | `/api/status` | Полное состояние камеры в JSON |
| `POST` | `/api/home` | Возврат в домашнее положение |
| `POST` | `/api/pan?deg=15.0` | Установка угла панорамы в градусах |
| `POST` | `/api/pan?dir=left&step=5.0` | Смещение влево |
| `POST` | `/api/pan?dir=right&step=5.0` | Смещение вправо |
| `POST` | `/api/tilt?deg=-10.0` | Установка угла наклона в градусах |
| `POST` | `/api/tilt?dir=up&step=5.0` | Смещение вверх |
| `POST` | `/api/tilt?dir=down&step=5.0` | Смещение вниз |
| `POST` | `/api/zoom?x=2.5` | Установка коэффициента зума |
| `POST` | `/api/zoom?dir=in&step=0.5` | Приближение |
| `POST` | `/api/zoom?dir=out&step=0.5` | Отдаление |
| `POST` | `/api/preset/recall/1` | Вызов аппаратного пресета 1..8 |
| `POST` | `/api/preset/save/1` | Сохранение аппаратного пресета 1..8 |
| `POST` | `/api/flicker?hz=50` | Установка частоты подавления мерцания (0, 50, 60) |
| `POST` | `/api/brightness?val=130` | Установка яркости |

---

## 🛠️ Внутреннее устройство и протоколы

- **V4L2 Controls (`VIDIOC_S_CTRL`, `VIDIOC_G_CTRL`)**:
  - `V4L2_CID_PAN_ABSOLUTE` (`0x009a0908`): значение угла в угловых секундах (3600 на 1°). Диапазон Logitech Rally: `-324000` (-90°) до `+324000` (+90°).
  - `V4L2_CID_TILT_ABSOLUTE` (`0x009a0909`): `-324000` (-90°) до `+180000` (+50°).
  - `V4L2_CID_ZOOM_ABSOLUTE` (`0x009a090d`): от `100` (1.0x) до `1500` (15.0x).
  - `V4L2_CID_POWER_LINE_FREQUENCY` (`0x00980918`): подавление мерцания (0 = выкл, 1 = 50Гц, 2 = 60Гц).
- **Logitech UVC Extension Unit (XU)**:
  - Unit ID 11 (`LOGITECH_XU_PERIPHERAL_CONTROL` `{ffe52d21-8030-4e2c-82d9-f587d00540bd}`):
    - Selector 2 (`PANTILT_MODE_CONTROL`):
      - `3`: Hardware Home
      - `4..11`: Сохранение в Hardware Presets 1..8
      - `12..19`: Вызов Hardware Presets 1..8
    - Selector 1 (`PANTILT_RELATIVE_CONTROL`): относительное импульсное управление моторами.
