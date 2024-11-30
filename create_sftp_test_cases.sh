# Standard-Uploads
mkdir -p ~/sftp_data/uploads && chmod -R 755 ~/sftp_data/uploads

# Read-only Ordner
mkdir -p ~/sftp_readonly && chmod -R 555 ~/sftp_readonly

# Write-only Ordner
mkdir -p ~/sftp_writeonly && chmod -R 333 ~/sftp_writeonly

# SSH-Schlüssel Ordner
mkdir -p ~/sshkeys && chmod -R 700 ~/sshkeys

# SSH-Schlüssel erstellen
ssh-keygen -t rsa -f ~/sshkeys/id_rsa -q -N ""
cp ~/sshkeys/id_rsa.pub ~/sshkeys/authorized_keys
chmod 600 ~/sshkeys/authorized_keys

# Logs-Ordner
mkdir -p ~/logs && chmod -R 755 ~/logs
