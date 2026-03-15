import axios from 'axios';

class ClaimClient {
  constructor() {
    this.baseURL = process.env.CLAIM_SERVER_URL || 'https://lensmint.onrender.com';
    const timeout = parseInt(process.env.CLAIM_CLIENT_TIMEOUT || '30000', 10);

    const headers = {
      'Content-Type': 'application/json'
    };

    if (process.env.API_SECRET) {
      headers['Authorization'] = `Bearer ${process.env.API_SECRET}`;
    }

    this.client = axios.create({
      baseURL: this.baseURL,
      timeout: timeout,
      headers: headers
    });
  }

  async createClaim(claim_id, cid, metadata_cid = null, device_id = null, camera_id = null, image_hash = null, signature = null, device_address = null) {
    try {
      const response = await this.client.post('/create-claim', {
        claim_id,
        cid,
        metadata_cid,
        device_id,
        camera_id,
        image_hash,
        signature,
        device_address
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to create claim');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async checkClaim(claim_id) {
    try {
      const response = await this.client.get('/check-claim', {
        params: { claim_id }
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to check claim');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async completeClaim(claim_id, tx_hash, token_id) {
    try {
      const response = await this.client.post('/complete-claim', {
        claim_id,
        tx_hash,
        token_id
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to complete claim');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async updateClaimStatus(claim_id, status, token_id = null, tx_hash = null) {
    try {
      const response = await this.client.post('/update-claim-status', {
        claim_id,
        status,
        token_id,
        tx_hash
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to update claim status');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async createEditionRequest(claim_id, wallet_address) {
    try {
      const response = await this.client.post('/create-edition-request', {
        claim_id,
        wallet_address
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to create edition request');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async getPendingEditionRequests(limit = 50) {
    try {
      const response = await this.client.get('/get-pending-edition-requests', {
        params: { limit }
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to get pending edition requests');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async updateEditionRequest(request_id, updates) {
    try {
      const response = await this.client.post('/update-edition-request', {
        request_id,
        ...updates
      });

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to update edition request');
      }
    } catch (error) {
      if (error.response) {
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async updateProofStatus(claim_id, token_id, verification_status, proof_tx_hash = null) {
    try {
      console.log(`[CLAIM CLIENT] Updating proof status:`, {
        claim_id,
        token_id,
        verification_status,
        proof_tx_hash: proof_tx_hash ? `${proof_tx_hash.substring(0, 10)}...` : null
      });

      const response = await this.client.post('/update-proof-status', {
        claim_id,
        token_id,
        verification_status,
        proof_tx_hash
      });

      console.log(`[CLAIM CLIENT] Response:`, response.data);

      if (response.data.success) {
        return response.data;
      } else {
        throw new Error(response.data.error || 'Failed to update proof status');
      }
    } catch (error) {
      console.error(`[CLAIM CLIENT] Error updating proof status:`, error.message);
      if (error.response) {
        console.error(`[CLAIM CLIENT] Response status:`, error.response.status);
        console.error(`[CLAIM CLIENT] Response data:`, error.response.data);
        throw new Error(error.response.data.error || 'Claim server error');
      }
      throw error;
    }
  }

  async healthCheck() {
    try {
      const response = await this.client.get('/health');
      return response.data;
    } catch (error) {
      return { status: 'error', message: error.message };
    }
  }
}

const claimClient = new ClaimClient();

export default claimClient;
