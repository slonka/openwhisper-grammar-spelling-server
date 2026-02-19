PYTHON ?= python3
SRC = server.py
BIN_NAME = openwhisper-cleanup-server
BUILD_DIR = build
DIST_DIR = dist
INSTALL_DIR = $(HOME)/bin
PLIST_NAME = com.openwhispr.cleanup-server
PLIST_DIR = $(HOME)/Library/LaunchAgents

.PHONY: build install uninstall launchd-install launchd-uninstall clean

build:
	$(PYTHON) -m nuitka \
		--standalone \
		--onefile \
		--output-filename=$(BIN_NAME) \
		--output-dir=$(BUILD_DIR) \
		--nofollow-import-to=torch \
		--nofollow-import-to=sympy \
		--include-package-data=langdetect \
		--include-package-data=language_tool_python \
		$(SRC)
	mkdir -p $(DIST_DIR)
	cp $(BUILD_DIR)/$(BIN_NAME) $(DIST_DIR)/$(BIN_NAME)
	@echo "Binary ready at $(DIST_DIR)/$(BIN_NAME)"

install: $(DIST_DIR)/$(BIN_NAME)
	mkdir -p $(INSTALL_DIR)
	cp $(DIST_DIR)/$(BIN_NAME) $(INSTALL_DIR)/$(BIN_NAME)
	@echo "Installed to $(INSTALL_DIR)/$(BIN_NAME)"

uninstall:
	rm -f $(INSTALL_DIR)/$(BIN_NAME)
	@echo "Removed $(INSTALL_DIR)/$(BIN_NAME)"

launchd-install: $(INSTALL_DIR)/$(BIN_NAME)
	mkdir -p $(PLIST_DIR)
	sed 's|__BIN__|$(INSTALL_DIR)/$(BIN_NAME)|g' $(PLIST_NAME).plist > $(PLIST_DIR)/$(PLIST_NAME).plist
	launchctl load $(PLIST_DIR)/$(PLIST_NAME).plist
	@echo "Launch agent loaded. Server will start on login."

launchd-uninstall:
	-launchctl unload $(PLIST_DIR)/$(PLIST_NAME).plist 2>/dev/null
	rm -f $(PLIST_DIR)/$(PLIST_NAME).plist
	@echo "Launch agent removed."

clean:
	rm -rf $(BUILD_DIR) $(DIST_DIR)
