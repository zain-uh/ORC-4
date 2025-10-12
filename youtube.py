import tkinter as tk
from tkinter import ttk
from PIL import Image, ImageTk
import requests
from io import BytesIO
import yt_dlp


def start(root, home_gui):
    """Launch the YouTube search interface with thumbnails."""

    # Clear main UI
    for widget in root.winfo_children():
        widget.destroy()

    # --- Main YouTube Frame ---
    frame = tk.Frame(root, bg="white")
    frame.pack(fill="both", expand=True)
    frame.focus_set()

    # --- Search Function ---
    def do_search():
        query = search_var.get().strip()
        for w in results_frame.winfo_children():
            w.destroy()

        if not query:
            tk.Label(results_frame, text="Please enter a search query.", bg="white").pack(anchor="w", padx=10, pady=10)
            return

        tk.Label(results_frame, text="Searching...", bg="white").pack(anchor="w", padx=10, pady=10)

        ydl_opts = {
            "quiet": True,
            "extract_flat": "in_playlist",
            "default_search": "ytsearch5",  # top 5 results
        }

        with yt_dlp.YoutubeDL(ydl_opts) as ydl:
            search_results = ydl.extract_info(query, download=False)

        for w in results_frame.winfo_children():
            w.destroy()

        if "entries" in search_results:
            for entry in search_results["entries"]:
                title = entry.get("title", "Unknown Title")
                url = entry.get("url", "")
                thumbnail_url = entry.get("thumbnails", [{}])[-1].get("url", "")

                # --- Thumbnail Download ---
                thumb_img = None
                if thumbnail_url:
                    try:
                        response = requests.get(thumbnail_url, timeout=5)
                        img_data = Image.open(BytesIO(response.content))
                        img_data = img_data.resize((160, 90))
                        thumb_img = ImageTk.PhotoImage(img_data)
                    except Exception:
                        pass

                # --- Each Result ---
                item_frame = tk.Frame(results_frame, bg="white", padx=10, pady=8)
                item_frame.pack(fill="x", anchor="w")

                if thumb_img:
                    thumb_label = tk.Label(item_frame, image=thumb_img, bg="white")
                    thumb_label.image = thumb_img  # prevent garbage collection
                    thumb_label.pack(side="left", padx=(0, 10))

                text_frame = tk.Frame(item_frame, bg="white")
                text_frame.pack(side="left", fill="x")

                title_label = tk.Label(text_frame, text=title, bg="white", fg="black", font=("Arial", 12, "bold"), wraplength=400, justify="left")
                title_label.pack(anchor="w")

                link_label = tk.Label(text_frame, text=url, bg="white", fg="blue", cursor="hand2", wraplength=400, justify="left")
                link_label.pack(anchor="w")
                link_label.bind("<Button-1>", lambda e, link=url: open_link(link))

        else:
            tk.Label(results_frame, text="No results found.", bg="white").pack(anchor="w", padx=10, pady=10)

        results_frame.update_idletasks()
        canvas.config(scrollregion=canvas.bbox("all"))

    # --- Open Link ---
    def open_link(url):
        import webbrowser
        webbrowser.open(url)

    # --- Back Button Function ---
    def go_home(event=None):
        """Return to main GUI."""
        for widget in root.winfo_children():
            widget.destroy()
        home_gui(root)

    # --- Key Bindings ---
    frame.bind_all("<Down>", go_home)

    # --- Scroll Update ---
    def on_frame_configure(event):
        canvas.configure(scrollregion=canvas.bbox("all"))

    # --- Header / Logo ---
    logo = tk.Label(frame, text="YouTube", font=("Arial", 24, "bold"), fg="red", bg="white")
    logo.pack(pady=10)

    # --- Search Bar + Buttons Frame ---
    top_frame = tk.Frame(frame, bg="white")
    top_frame.pack(pady=5)

    # Search Entry
    search_var = tk.StringVar()
    search_entry = tk.Entry(top_frame, textvariable=search_var, font=("Arial", 14), width=30)
    search_entry.pack(side="left", padx=(0, 5))

    # Search Button
    search_btn = tk.Button(top_frame, text="Search", command=do_search)
    search_btn.pack(side="left", padx=(0, 5))

    # Home Button
    home_btn = tk.Button(top_frame, text="Home", command=go_home)
    home_btn.pack(side="left")

    # --- Scrollable Results Area ---
    canvas = tk.Canvas(frame, bg="white", highlightthickness=0)
    scrollbar = ttk.Scrollbar(frame, orient="vertical", command=canvas.yview)
    results_frame = tk.Frame(canvas, bg="white")

    canvas.create_window((0, 0), window=results_frame, anchor="nw")
    canvas.configure(yscrollcommand=scrollbar.set)

    canvas.pack(side="left", fill="both", expand=True)
    scrollbar.pack(side="right", fill="y")

    results_frame.bind("<Configure>", on_frame_configure)