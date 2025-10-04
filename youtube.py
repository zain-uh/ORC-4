import tkinter as tk
import yt_dlp

def start(root):
    # Clear existing UI
    for widget in root.winfo_children():
        widget.destroy()

    # Frame for YouTube
    frame = tk.Frame(root, bg="white")
    frame.pack(fill="both", expand=True)

    # Logo (placeholder text for now)
    logo = tk.Label(frame, text="YouTube", font=("Arial", 24, "bold"), fg="red", bg="white")
    logo.pack(pady=10)

    # Search bar
    search_var = tk.StringVar()
    search_entry = tk.Entry(frame, textvariable=search_var, font=("Arial", 14), width=30)
    search_entry.pack(pady=5)


    # Output label for now (we’ll make a proper results list later)
    results_label = tk.Label(frame, text="", bg="white", justify="left", anchor="w")
    results_label.pack(pady=10)
    # Search button
    def do_search():
        query = search_var.get()
        if not query:
            results_label.config(text="Please enter a search query.")
            return
        results_label.config(text="Searching...")
        # Use yt_dlp to search YouTube
        ydl_opts = {
            'quiet': True,
            'extract_flat': 'in_playlist',
            'default_search': 'ytsearch5',  # get top 5 results CAN CHANGE
        }
        with yt_dlp.YoutubeDL(ydl_opts) as ydl:
            search_results = ydl.extract_info(query, download=False)

        # Build a readable list
        if 'entries' in search_results:
            results_text = ""
            for idx, entry in enumerate(search_results['entries'], 1):
                title = entry.get('title', 'Unknown Title')
                url = entry.get('url', '')
                results_text += f"{idx}. {title}\nhttps://www.youtube.com/watch?v={url}\n\n"
            results_label.config(text=results_text)
        else:
            results_label.config(text="No results found.")

    search_btn = tk.Button(frame, text="Search", command=do_search)
    search_btn.pack(pady=5)
    
    
    def go_back(event=None):
        root.destroy()  # Or, better: call a callback to restore the main GUI
        # NOT GOOD CHANGE SOON
    back_btn = tk.Button(frame, text="Back", command=go_back)
    back_btn.pack(pady=20)

    frame.bind_all('<Down>', go_back)  # Bind Down arrow to go_back

    # # Back button to return to gui
    # from gui import open_youtube  # careful with circular imports
    # back_btn = tk.Button(frame, text="Back", command=lambda: root.quit())  # placeholder
    # back_btn.pack(pady=20)