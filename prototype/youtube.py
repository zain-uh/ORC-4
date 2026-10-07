"""
YouTube search UI — white canvas, 2×2 grid; search bar only on YT home.
On YT search home: Esc → main ORC home (home.py). On results grid: Esc → YT search home.
Arrow keys move in the grid. B → main ORC home from results.
"""
import tkinter as tk
from tkinter import Canvas, Frame, Entry, Button
import webbrowser

import requests
from io import BytesIO
import yt_dlp
from PIL import Image, ImageTk

WINDOW_W = 960
WINDOW_H = 540
BG = "#FFFFFF"
TEXT_COLOR = "#282828"
ACCENT = "#FF0000"

THUMB_W = 300
THUMB_H = int(THUMB_W * 9 / 16)
RESULTS_PER_PAGE = 4
# Larger pixel sizes for small / high-DPI screens (Tk negative size = points in pixels)
TITLE_FONT = ("IBM Plex Mono SemiBold", -22)
HEADER_FONT = ("IBM Plex Mono SemiBold", -36)
STATUS_FONT = ("IBM Plex Mono SemiBold", -22)
ARROW_HINT_FONT = ("IBM Plex Mono SemiBold", -48)
ENTRY_FONT = ("IBM Plex Mono SemiBold", -22)
BTN_FONT = ("IBM Plex Mono SemiBold", -20)

TITLE_MAX_CHARS = 40
TITLE_WRAP = THUMB_W - 4

# First batch size; grows when loading more (dedupe by video id)
SEARCH_INITIAL = 16
SEARCH_MORE_STEP = 16
SEARCH_CAP = 50

try:
    _RESAMPLE = Image.Resampling.LANCZOS
except AttributeError:
    _RESAMPLE = Image.LANCZOS

# Grid vertical center offset (negative = move whole grid up)
GRID_OFFSET_Y = -28


def _truncate_title(text: str) -> str:
    t = (text or "Unknown").strip() or "Unknown"
    if len(t) <= TITLE_MAX_CHARS:
        return t
    return t[: TITLE_MAX_CHARS - 1].rstrip() + "…"


def _entry_id(entry: dict) -> str:
    return str(entry.get("id") or entry.get("url") or entry.get("title") or "")


def _sel_to_rc(sel: int, m: int):
    """Map linear index on page to (row, col) for 2×2 layout."""
    if m <= 0:
        return None
    if m == 1:
        return (0, 0)
    if m == 2:
        return (0, sel)
    if m == 3:
        if sel <= 1:
            return (0, sel)
        return (1, 0)
    return (sel // 2, sel % 2)


def _rc_to_sel(row: int, col: int, m: int):
    if m <= 0:
        return None
    if m == 1:
        return 0 if (row, col) == (0, 0) else None
    if m == 2:
        return col if row == 0 and 0 <= col <= 1 else None
    if m == 3:
        if row == 0 and col == 0:
            return 0
        if row == 0 and col == 1:
            return 1
        if row == 1 and col == 0:
            return 2
        return None
    if m == 4:
        if 0 <= row <= 1 and 0 <= col <= 1:
            return row * 2 + col
    return None


def start(root, home_gui):
    for widget in root.winfo_children():
        widget.destroy()

    state = {
        "entries": [],
        "page": 0,
        "sel": 0,
        "photos": [],
        "item_ids": [],
        "highlight_id": None,
        "search_win_id": None,
        "last_query": "",
        "fetch_cap": SEARCH_INITIAL,
    }

    main = Canvas(
        root,
        bg=BG,
        width=WINDOW_W,
        height=WINDOW_H,
        bd=0,
        highlightthickness=0,
        relief="ridge",
    )
    main.place(x=0, y=0)

    def hide_loading_overlay():
        try:
            main.delete("loading")
        except tk.TclError:
            pass

    def show_loading_overlay(msg: str = "Loading…"):
        """Dim the canvas and show a message (call root.update() after to paint)."""
        hide_loading_overlay()
        try:
            main.create_rectangle(
                0,
                0,
                WINDOW_W,
                WINDOW_H,
                tags="loading",
                fill=BG,
                stipple="gray25",
                outline="",
            )
        except tk.TclError:
            main.create_rectangle(
                0,
                0,
                WINDOW_W,
                WINDOW_H,
                tags="loading",
                fill="#E8E8E8",
                outline="",
            )
        main.create_text(
            WINDOW_W // 2,
            WINDOW_H // 2,
            text=msg,
            fill=TEXT_COLOR,
            font=HEADER_FONT,
            tags="loading",
        )
        main.tag_raise("loading")

    def clear_result_items():
        for iid in state["item_ids"]:
            try:
                main.delete(iid)
            except tk.TclError:
                pass
        state["item_ids"].clear()
        state["highlight_id"] = None

    def set_search_bar_visible(visible: bool):
        wid = state.get("search_win_id")
        if wid is None:
            return
        try:
            main.itemconfigure(wid, state="normal" if visible else "hidden")
        except tk.TclError:
            pass

    def layout_positions(count: int):
        if count <= 0:
            return []
        gap_x = 24
        gap_y = 16
        title_room = 64
        col_step = THUMB_W + gap_x
        row_step = THUMB_H + title_room + gap_y

        mid_x = WINDOW_W // 2
        mid_y = WINDOW_H // 2 + GRID_OFFSET_Y

        cx_left = mid_x - col_step // 2
        cx_right = mid_x + col_step // 2
        cy_top = mid_y - row_step // 2
        cy_bot = mid_y + row_step // 2

        slots = [
            (cx_left, cy_top),
            (cx_right, cy_top),
            (cx_left, cy_bot),
            (cx_right, cy_bot),
        ]
        if count == 1:
            return [(mid_x, mid_y)]
        if count == 2:
            return [slots[0], slots[1]]
        if count == 3:
            return [slots[0], slots[1], slots[2]]
        return slots[:4]

    def draw_status(msg: str, *, error: bool = False):
        """Hint text below the centered search bar (YT home)."""
        clear_result_items()
        tid = main.create_text(
            WINDOW_W // 2,
            WINDOW_H // 2 + 120,
            text=msg,
            fill="#B71C1C" if error else TEXT_COLOR,
            font=STATUS_FONT,
            width=WINDOW_W - 80,
            justify="center",
        )
        state["item_ids"].append(tid)

    def draw_highlight(cx, cy):
        if state["highlight_id"] is not None:
            try:
                main.delete(state["highlight_id"])
            except tk.TclError:
                pass
        pad = 6
        x1 = cx - THUMB_W // 2 - pad
        y1 = cy - THUMB_H // 2 - pad
        x2 = cx + THUMB_W // 2 + pad
        y2 = cy + THUMB_H // 2 + pad
        state["highlight_id"] = main.create_rectangle(
            x1, y1, x2, y2, outline=TEXT_COLOR, width=4
        )
        state["item_ids"].append(state["highlight_id"])
        main.tag_raise(state["highlight_id"])

    def _build_entry_dict(entry: dict) -> dict | None:
        if not entry:
            return None
        title = entry.get("title", "Unknown Title")
        url = entry.get("url") or ""
        thumbnail_url = ""
        thumbs = entry.get("thumbnails") or []
        if thumbs:
            thumbnail_url = thumbs[-1].get("url", "")
        photo = None
        if thumbnail_url:
            try:
                response = requests.get(thumbnail_url, timeout=8)
                response.raise_for_status()
                img_data = Image.open(BytesIO(response.content)).convert("RGB")
                img_data = img_data.resize((THUMB_W, THUMB_H), _RESAMPLE)
                photo = ImageTk.PhotoImage(img_data)
                state["photos"].append(photo)
            except Exception:
                photo = None
        vid = entry.get("id") or ""
        return {"title": title, "url": url, "id": vid, "photo": photo}

    def fetch_more_results() -> bool:
        """Larger ytsearch pull; append entries not seen yet. Returns True if any added."""
        query = (state.get("last_query") or "").strip()
        if not query:
            return False
        cap = min(state["fetch_cap"] + SEARCH_MORE_STEP, SEARCH_CAP)
        state["fetch_cap"] = cap
        ydl_opts = {
            "quiet": True,
            "extract_flat": "in_playlist",
            "default_search": f"ytsearch{cap}",
        }
        try:
            with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                info = ydl.extract_info(query, download=False)
        except Exception:
            return False
        raw = info.get("entries") or []
        seen = {_entry_id(e) for e in state["entries"]}
        added = False
        for entry in raw:
            if not entry:
                continue
            row = _build_entry_dict(entry)
            if not row:
                continue
            key = _entry_id(row)
            if not key or key in seen:
                continue
            seen.add(key)
            state["entries"].append(row)
            added = True
        return added

    def ensure_index_reachable(first_index: int) -> None:
        """Fetch more (larger ytsearch + dedupe) until len > first_index or exhausted."""
        if len(state["entries"]) > first_index or not state.get("last_query"):
            return
        show_loading_overlay("Loading more…")
        root.update_idletasks()
        root.update()
        tries = 0
        while len(state["entries"]) <= first_index and state.get("last_query") and tries < 10:
            if not fetch_more_results():
                break
            tries += 1
            root.update_idletasks()
        hide_loading_overlay()

    def render_page():
        clear_result_items()
        entries = state["entries"]
        if not entries:
            draw_status("Search to see results.")
            return

        n = len(entries)
        start_i = state["page"] * RESULTS_PER_PAGE
        page_slice = entries[start_i : start_i + RESULTS_PER_PAGE]
        m = len(page_slice)
        if m == 0:
            last_page = max(0, (n - 1) // RESULTS_PER_PAGE)
            state["page"] = min(state["page"], last_page)
            render_page()
            return

        state["sel"] = max(0, min(state["sel"], m - 1))
        positions = layout_positions(m)
        mid_y = WINDOW_H // 2 + GRID_OFFSET_Y

        state["item_ids"].append(
            main.create_text(
                24,
                mid_y,
                anchor="w",
                text="<",
                fill=TEXT_COLOR,
                font=ARROW_HINT_FONT,
                tags="ytres",
            )
        )
        state["item_ids"].append(
            main.create_text(
                WINDOW_W - 24,
                mid_y,
                anchor="e",
                text=">",
                fill=TEXT_COLOR,
                font=ARROW_HINT_FONT,
                tags="ytres",
            )
        )

        for idx, ent in enumerate(page_slice):
            cx, cy = positions[idx]
            photo = ent.get("photo")
            if photo:
                state["item_ids"].append(
                    main.create_image(cx, cy, image=photo, tags="ytres")
                )
            else:
                state["item_ids"].append(
                    main.create_rectangle(
                        cx - THUMB_W // 2,
                        cy - THUMB_H // 2,
                        cx + THUMB_W // 2,
                        cy + THUMB_H // 2,
                        outline="#CCCCCC",
                        width=2,
                        tags="ytres",
                    )
                )
            title = _truncate_title(ent.get("title"))
            state["item_ids"].append(
                main.create_text(
                    cx,
                    cy + THUMB_H // 2 + 10,
                    text=title,
                    fill=TEXT_COLOR,
                    font=TITLE_FONT,
                    width=TITLE_WRAP,
                    justify="center",
                    anchor="n",
                    tags="ytres",
                )
            )

        cx, cy = positions[state["sel"]]
        draw_highlight(cx, cy)
        main.tag_raise(state["highlight_id"])

    def snap_selection_to_cell(m: int):
        """Move the focus rectangle instantly (no animation)."""
        positions = layout_positions(m)
        sel = max(0, min(state["sel"], len(positions) - 1))
        state["sel"] = sel
        cx, cy = positions[sel]
        pad = 6
        x1 = cx - THUMB_W // 2 - pad
        y1 = cy - THUMB_H // 2 - pad
        x2 = cx + THUMB_W // 2 + pad
        y2 = cy + THUMB_H // 2 + pad
        hid = state.get("highlight_id")
        if hid is not None:
            try:
                main.coords(hid, x1, y1, x2, y2)
                main.tag_raise(hid)
                return
            except tk.TclError:
                pass
        draw_highlight(cx, cy)
        main.tag_raise(state["highlight_id"])

    def page_context():
        entries = state["entries"]
        if not entries:
            return None
        start_i = state["page"] * RESULTS_PER_PAGE
        page_slice = entries[start_i : start_i + RESULTS_PER_PAGE]
        m = len(page_slice)
        if m == 0:
            return None
        return entries, start_i, page_slice, m

    def try_move_spatial(dcol: int, drow: int):
        ctx = page_context()
        if not ctx:
            return
        _entries, _start_i, _page_slice, m = ctx
        rc = _sel_to_rc(state["sel"], m)
        if rc is None:
            return
        row, col = rc
        nrow, ncol = row + drow, col + dcol
        new_sel = _rc_to_sel(nrow, ncol, m)

        if new_sel is not None:
            state["sel"] = new_sel
            snap_selection_to_cell(m)
            return

        all_e = state["entries"]
        n_all = len(all_e)

        # Horizontal: previous / next page
        if drow == 0 and dcol != 0:
            if dcol < 0 and col == 0 and state["page"] > 0:
                state["page"] -= 1
                prev_start = state["page"] * RESULTS_PER_PAGE
                prev_m = min(RESULTS_PER_PAGE, n_all - prev_start)
                state["sel"] = max(0, prev_m - 1)
                render_page()
                return
            if dcol > 0:
                # No cell to the right on this page → next page (fetch after ~4 pages need more rows)
                at_right = _rc_to_sel(row, col + 1, m) is None
                if at_right:
                    next_page = state["page"] + 1
                    next_start = next_page * RESULTS_PER_PAGE
                    ensure_index_reachable(next_start)
                    if next_start < len(state["entries"]):
                        state["page"] = next_page
                        state["sel"] = 0
                        render_page()
                return

        # Vertical: same column on adjacent row, else previous/next page
        if dcol == 0 and drow != 0:
            if drow < 0 and row == 0:
                if state["page"] > 0:
                    state["page"] -= 1
                    ps = state["page"] * RESULTS_PER_PAGE
                    pm = min(RESULTS_PER_PAGE, n_all - ps)
                    land = _rc_to_sel(1, col, pm)
                    state["sel"] = land if land is not None else max(0, pm - 1)
                    render_page()
                return
            if drow > 0 and _rc_to_sel(row + 1, col, m) is None:
                next_page = state["page"] + 1
                ns = next_page * RESULTS_PER_PAGE
                ensure_index_reachable(ns)
                if ns < len(state["entries"]):
                    state["page"] = next_page
                    nm = min(RESULTS_PER_PAGE, len(state["entries"]) - ns)
                    land = _rc_to_sel(0, col, nm)
                    state["sel"] = land if land is not None else 0
                    render_page()
                return

    def on_left(event=None):
        try_move_spatial(-1, 0)
        return "break"

    def on_right(event=None):
        try_move_spatial(1, 0)
        return "break"

    def on_up(event=None):
        try_move_spatial(0, -1)
        return "break"

    def on_down(event=None):
        try_move_spatial(0, 1)
        return "break"

    def on_enter(event=None):
        entries = state["entries"]
        if not entries:
            return "break"
        start_i = state["page"] * RESULTS_PER_PAGE
        idx = start_i + state["sel"]
        if 0 <= idx < len(entries):
            ent = entries[idx]
            url = ent.get("url") or ""
            vid = ent.get("id") or ""
            if not url and vid:
                url = f"https://www.youtube.com/watch?v={vid}"
            elif url and not url.startswith("http"):
                url = "https://www.youtube.com/watch?v=" + url
            if url:
                webbrowser.open(url)
        return "break"

    def go_main_home(event=None):
        hide_loading_overlay()
        for widget in root.winfo_children():
            widget.destroy()
        home_gui(root)

    def go_yt_home(event=None):
        hide_loading_overlay()
        state["entries"] = []
        state["photos"] = []
        state["page"] = 0
        state["sel"] = 0
        state["last_query"] = ""
        state["fetch_cap"] = SEARCH_INITIAL
        clear_result_items()
        set_search_bar_visible(True)
        draw_status("Type a search, then Enter or Search.")
        search_entry.focus_set()
        return "break"

    def on_escape(event=None):
        """YT search home (no results): Esc → main ORC home. On grid: Esc → YT search home."""
        if state.get("entries"):
            return go_yt_home(event)
        return go_main_home(event)

    top_f = Frame(main, bg=BG)
    tk.Label(top_f, text="YouTube", fg=ACCENT, bg=BG, font=HEADER_FONT).pack(
        pady=(0, 10)
    )
    search_row = Frame(top_f, bg=BG)
    search_row.pack()
    search_var = tk.StringVar()
    search_entry = Entry(
        search_row,
        textvariable=search_var,
        font=ENTRY_FONT,
        width=26,
    )
    search_entry.pack(side="left", padx=6)

    def do_search(event=None):
        query = search_var.get().strip()
        state["entries"] = []
        state["photos"] = []
        state["page"] = 0
        state["sel"] = 0
        state["last_query"] = query
        state["fetch_cap"] = SEARCH_INITIAL
        clear_result_items()
        set_search_bar_visible(True)
        if not query:
            draw_status("Type a search, then Enter or Search.")
            return "break"

        clear_result_items()
        show_loading_overlay("Searching…")
        root.update_idletasks()
        root.update()

        ydl_opts = {
            "quiet": True,
            "extract_flat": "in_playlist",
            "default_search": f"ytsearch{SEARCH_INITIAL}",
        }

        try:
            with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                search_results = ydl.extract_info(query, download=False)
        except Exception as exc:
            hide_loading_overlay()
            clear_result_items()
            draw_status(f"Search failed.\n{exc}", error=True)
            set_search_bar_visible(True)
            return "break"

        raw = search_results.get("entries") or []
        if not raw:
            hide_loading_overlay()
            clear_result_items()
            draw_status("No results found.")
            set_search_bar_visible(True)
            return "break"

        show_loading_overlay("Loading thumbnails…")
        root.update_idletasks()
        root.update()

        built = []
        for entry in raw:
            row = _build_entry_dict(entry)
            if row:
                built.append(row)
            root.update_idletasks()

        hide_loading_overlay()
        state["entries"] = built
        state["page"] = 0
        state["sel"] = 0
        set_search_bar_visible(False)
        render_page()
        main.focus_set()
        return "break"

    search_btn = Button(search_row, text="Search", command=do_search, font=BTN_FONT, padx=12, pady=6)
    search_btn.pack(side="left", padx=4)

    win_id = main.create_window(
        WINDOW_W // 2,
        WINDOW_H // 2,
        window=top_f,
        anchor="center",
    )
    state["search_win_id"] = win_id

    search_entry.bind("<Return>", do_search)
    search_entry.bind("<Escape>", on_escape)
    search_btn.bind("<Escape>", on_escape)
    top_f.bind("<Escape>", on_escape)

    main.bind("<Left>", on_left)
    main.bind("<Right>", on_right)
    main.bind("<Up>", on_up)
    main.bind("<Down>", on_down)
    main.bind("<Return>", on_enter)
    main.bind("<Escape>", on_escape)
    main.bind("b", go_main_home)
    main.bind("B", go_main_home)

    draw_status("Type a search, then Enter or Search.")
    search_entry.focus_set()
