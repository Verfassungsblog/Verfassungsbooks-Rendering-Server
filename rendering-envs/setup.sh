#/bin/sh

# Prepares the environments for pandoc & vivliostyle

# Pandoc:
# Überprüfen, ob das Verzeichnis "pandoc" existiert
if [ -d "pandoc" ]; then
  # Initialer Wert für den Zähler
  count=1
  
  # Finde den nächsten verfügbaren Namen für pandoc-old-x
  while [ -d "pandoc-old-$count" ]; do
    count=$((count + 1))
  done
  
  # Verschiebe das vorhandene pandoc-Verzeichnis
  mv pandoc "pandoc-old-$count"
fi

# Erstelle das neue pandoc-Verzeichnis
mkdir pandoc

latest_release=$(curl --silent "https://api.github.com/repos/jgm/pandoc/releases/latest" | grep "browser_download_url.*linux-amd64.tar.gz" | cut -d '"' -f 4)

# Prüfen, ob eine URL gefunden wurde
if [ -z "$latest_release" ]; then
  echo "Error: couldn't extract download link to latest release."
  exit 1
fi

curl -L "$latest_release" -o pandoc/pandoc-latest.tar.gz

tar -xzf pandoc/pandoc-latest.tar.gz -C pandoc --strip-components=1

rm pandoc/pandoc-latest.tar.gz

mv pandoc/bin/pandoc pandoc/pandoc
rm -r pandoc/bin pandoc/share

echo "Finished preparing pandoc env."

# Vivliostyle

# Step 1: Check if the vivliostyle directory exists and rename it if necessary
if [ -d "vivliostyle" ]; then
  count=1
  while [ -d "vivliostyle-old-$count" ]; do
    count=$((count + 1))
  done
  mv vivliostyle "vivliostyle-old-$count"
fi

# Step 2: Create a new vivliostyle directory
mkdir vivliostyle

# Step 3: Create a subdirectory for building Node.js
mkdir vivliostyle/node-build

# Step 4: Download the latest stable version of Node.js using curl
NODE_VERSION=$(curl -sL https://nodejs.org/dist/latest/ | grep -oP 'node-v\K[0-9]+\.[0-9]+\.[0-9]+' | head -n 1)
NODE_TAR="node-v$NODE_VERSION.tar.gz"
NODE_URL="https://nodejs.org/dist/latest/$NODE_TAR"

# Download the Node.js source code into the node-build subdirectory
curl -o "vivliostyle/node-build/$NODE_TAR" "$NODE_URL"

# Step 5: Extract the downloaded archive into the node-build subdirectory
tar -xzf "vivliostyle/node-build/$NODE_TAR" -C vivliostyle/node-build --strip-components=1

# Step 6: Change to the node-build subdirectory
cd vivliostyle/node-build || exit 1

# Step 7: Configure the build for a statically linked binary
./configure --fully-static

# Step 8: Determine the number of CPU cores
num_jobs=$(nproc --ignore=1)

# Step 9: Build Node.js with the maximum number of jobs
make -j"$num_jobs"

# Step 10: Copy the newly created node binary to the vivliostyle directory
cp node ../node

# Step 11: Clean up by removing the node-build subdirectory
cd ..
rm -rf node-build

# Step 12: Install Vivliostyle CLI in the vivliostyle directory using the new node binary
npm install @vivliostyle/cli

rm package.json
rm package-lock.json
echo "Finished preparing vivliostyle env."

# Step 13: Create a subdirectory for Chromium
mkdir vivliostyle/chromium

# Step 14: Download the latest Chromium testing version using curl
CHROMIUM_VERSION=$(curl -sL https://www.googleapis.com/download/storage/v1/b/chromium-browser-snapshots/o/Linux_x64%2FLAST_CHANGE?alt=media)
CHROMIUM_TAR="chrome-linux-$CHROMIUM_VERSION.zip"
CHROMIUM_URL="https://www.googleapis.com/download/storage/v1/b/chromium-browser-snapshots/o/Linux_x64%2F$CHROMIUM_VERSION%2Fchrome-linux.zip?alt=media"

# Download Chromium testing version into the chromium subdirectory
curl -o "vivliostyle/chromium/$CHROMIUM_TAR" "$CHROMIUM_URL"

# Step 15: Extract the downloaded Chromium archive into the chromium subdirectory
unzip -q "vivliostyle/chromium/$CHROMIUM_TAR" -d vivliostyle/chromium
mv vivliostyle/chromium/chrome-linux/* vivliostyle/chromium/
rmdir vivliostyle/chromium/chrome-linux

# Step 16: Clean up by removing the Chromium zip file
rm "vivliostyle/chromium/$CHROMIUM_TAR"

echo "Finished preparing vivliostyle and chromium env."